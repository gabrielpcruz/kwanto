# Ambiente de compilação dos pacotes: Ubuntu 22.04, para os binários rodarem em glibc 2.35+
# (Ubuntu 22.04, Debian 12, Fedora e derivados). Imagem, instalador do Rust e versão do Rust fixados
# por hash/versão: a mesma tag compila sempre com as mesmas ferramentas.
FROM ubuntu:22.04@sha256:5ec03bb3441e8b0bf3b4f9cd4629a1ae763010dc3035bb8da3ae6cf026486401
RUN apt-get update -qq \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
      ca-certificates curl build-essential pkg-config \
      libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev \
 && rm -rf /var/lib/apt/lists/*
# Em /opt, legível por qualquer usuário: o container roda com o usuário de quem compila.
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
ARG RUSTUP=1.29.1
ARG RUSTUP_SHA256=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
RUN curl -fsSL -o /tmp/rustup-init "https://static.rust-lang.org/rustup/archive/${RUSTUP}/x86_64-unknown-linux-gnu/rustup-init" \
 && echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
 && chmod +x /tmp/rustup-init \
 && /tmp/rustup-init -y --profile minimal --default-toolchain 1.99.0 --component clippy,rustfmt --no-modify-path \
 && rm /tmp/rustup-init \
 && chmod -R a+rX /opt/rustup /opt/cargo
WORKDIR /src
