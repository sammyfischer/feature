FROM docker.io/library/rust:latest

RUN apt-get update && \
  apt-get install -y git && \
  apt-get install -y less

WORKDIR /workspace
