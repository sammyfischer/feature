use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use git2::{Oid, RemoteCallbacks};
use indicatif::{BinaryBytes, HumanCount, ProgressBar, ProgressStyle};

use crate::core::fetch::get_credentials_cb;
use crate::core::string::ToStrLossy;
use crate::core::term::{PROGRESS_CHARS, TICK_STRINGS};

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

/// Gets fully configured push callbacks. This creates and begins ticking a
/// progress bar, so callbacks should be obtained close to when the actual push
/// is performed.
///
/// # Params
/// - `status` - the [PushStatus] structure to hold the results of the push
pub fn get_push_callbacks<'cbs, 'repo: 'cbs>(
  status: &'cbs mut PushStatus,
) -> Result<RemoteCallbacks<'cbs>> {
  let mut cbs = RemoteCallbacks::new();
  cbs.credentials(get_credentials_cb());

  let transfer_progress = ProgressBar::new(0).with_style(
    ProgressStyle::with_template("{spinner:.cyan} {elapsed} [{bar:40.cyan}] {msg}")?
      .progress_chars(PROGRESS_CHARS)
      .tick_strings(&TICK_STRINGS),
  );
  transfer_progress.enable_steady_tick(Duration::from_millis(100));

  cbs.push_transfer_progress(move |current, total, bytes| {
    if transfer_progress.length().is_none() || transfer_progress.length() == Some(0) {
      transfer_progress.set_length(total as u64);
    }

    transfer_progress.set_position(current as u64);

    if current != total {
      transfer_progress.set_message(format!("Transferring {}/{} objects", current, total));
    } else {
      transfer_progress.finish_with_message(format!(
        "Transferred {} objects ({})",
        HumanCount(total as u64),
        BinaryBytes(bytes as u64)
      ));
    }
  });

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
