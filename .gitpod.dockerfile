FROM gitpod/workspace-full:latest

USER gitpod
ENV PATH="${PATH}:${HOME}/.cargo/bin"
RUN rustup toolchain remove stable # Work around a docker/gitpod bug
RUN rustup toolchain add stable beta nightly
RUN rustup component add rls clippy rustfmt llvm-tools-preview rust-src rust-docs rust-analysis
RUN rustup update

USER root
RUN apt-get update
RUN apt-get install -y clang
