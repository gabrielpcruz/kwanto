# Ambiente limpo para scripts/verificar-limpo.sh: Ubuntu 24.04 como o runner do CI, só com o que o
# CI instala. Nada da máquina de quem desenvolve entra aqui.
FROM ubuntu:24.04@sha256:534baea6a22c03a63003dbc8dbe78fe34bc0d7e595d9a9dc9834884ff530eb55
RUN apt-get update -qq \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
      ca-certificates curl git build-essential pkg-config python3 dbus systemd \
      libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev \
 && curl -fsSL -o /tmp/chrome.deb https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq /tmp/chrome.deb \
 && rm -rf /tmp/chrome.deb /var/lib/apt/lists/*
# Rust como no CI: rustup lê a versão em rust-toolchain.toml.
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
ARG RUSTUP=1.29.1
ARG RUSTUP_SHA256=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
RUN curl -fsSL -o /tmp/rustup-init "https://static.rust-lang.org/rustup/archive/${RUSTUP}/x86_64-unknown-linux-gnu/rustup-init" \
 && echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
 && chmod +x /tmp/rustup-init \
 && /tmp/rustup-init -y --profile minimal --default-toolchain 1.99.0 --component clippy,rustfmt --no-modify-path \
 && rm /tmp/rustup-init && chmod -R a+rwX /opt/rustup /opt/cargo
