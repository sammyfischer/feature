use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use git2::{Oid, RemoteCallbacks};

use crate::core::fetch::get_credentials_cb;
use crate::core::string::ToStrLossy;

pub mod check;

/// The results of a push. This includes each ref that was updated or rejected,
/// and the arbitrary text sent by the server.
pub struct PushStatus {
  /// Each branch that was updated in the local repository
  updates: Rc<RefCell<Vec<PushUpdate>>>,

  /// Each branch that failed to push
  rejections: Rc<RefCell<Vec<PushRejection>>>,

  /// Arbitrary server reponse
  server: Rc<RefCell<String>>,
}

pub struct PushUpdate {
  pub refname: String,
  pub kind: PushUpdateKind,
}

pub enum PushUpdateKind {
  Create(Oid),
  Update(Oid, Oid),
  Delete(Oid),
}

pub struct PushRejection {
  pub refname: String,
  pub status: String,
}

impl PushStatus {
  pub fn new() -> Self {
    PushStatus {
      updates: Rc::new(RefCell::new(Vec::new())),
      rejections: Rc::new(RefCell::new(Vec::new())),
      server: Rc::new(RefCell::new(String::new())),
    }
  }

  /// Consumes this [PushStatus], returning the output structures
  pub fn into_inner(self) -> (Vec<PushUpdate>, Vec<PushRejection>, String) {
    (
      Rc::into_inner(self.updates)
        .unwrap_or_default()
        .into_inner(),
      Rc::into_inner(self.rejections)
        .unwrap_or_default()
        .into_inner(),
      Rc::into_inner(self.server).unwrap_or_default().into_inner(),
    )
  }
}

/// Gets fully configured push callbacks.
///
/// # Params
/// - `status` - the [PushStatus] structure to hold the results of the push
pub fn get_push_callbacks<'cbs, 'repo: 'cbs>(
  status: &'cbs mut PushStatus,
) -> Result<RemoteCallbacks<'cbs>> {
  let mut cbs = RemoteCallbacks::new();
  cbs.credentials(get_credentials_cb());

  // called on each remote tracking branch that's updated
  let updates = status.updates.clone();
  cbs.update_tips(move |name: &str, old_id: Oid, new_id: Oid| -> bool {
    if old_id == new_id {
      return true;
    }

    let zero = Oid::ZERO_SHA1;

    match (old_id, new_id) {
      (old, new) if old == zero && new != zero => {
        updates.borrow_mut().push(PushUpdate {
          refname: name.to_string(),
          kind: PushUpdateKind::Create(new),
        });
      }

      (old, new) if new == zero && old != zero => {
        updates.borrow_mut().push(PushUpdate {
          refname: name.to_string(),
          kind: PushUpdateKind::Delete(old),
        });
      }

      (old, new) => {
        updates.borrow_mut().push(PushUpdate {
          refname: name.to_string(),
          kind: PushUpdateKind::Update(old, new),
        });
      }
    }

    true
  });

  // print error if push fails
  let rejection_buf = status.rejections.clone();
  cbs.push_update_reference(move |refname, status| {
    // a status of Some means push was rejected
    if let Some(msg) = status {
      rejection_buf.borrow_mut().push(PushRejection {
        refname: refname.to_string(),
        status: msg.to_string(),
      });
      return Err(git2::Error::from_str(msg));
    }
    Ok(())
  });

  // this is arbitrary text sent by the server. on github/gitlab, this usually
  // contains info on how to create a pull request for newly pushed branches
  use std::fmt::Write;
  let response_buf = status.server.clone();
  cbs.sideband_progress(move |bytes| {
    let _ = write!(response_buf.borrow_mut(), "{}", bytes.to_str_lossy());
    true
  });

  Ok(cbs)
}
