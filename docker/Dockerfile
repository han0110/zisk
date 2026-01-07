FROM nvidia/cuda:12.9.1-devel-ubuntu24.04 AS builder

# Taken from https://0xpolygonhermez.github.io/zisk/getting_started/installation.html
RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    xz-utils \
    jq \
    curl \
    build-essential \
    qemu-system \
    libomp-dev \
    libgmp-dev \
    nlohmann-json3-dev \
    protobuf-compiler \
    uuid-dev \
    libgrpc++-dev \
    libsecp256k1-dev \
    libsodium-dev \
    libpqxx-dev \
    nasm \
    libopenmpi-dev \
    openmpi-bin \
    openmpi-common \
    libclang-dev \
    clang \
    gcc-riscv64-unknown-elf \
    libprotobuf-dev \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# Install rustup
ARG RUST_VERSION=1.88.0

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain ${RUST_VERSION} --no-modify-path

# Default to build for RTX 50 series
ARG CUDA_ARCH=sm_120

# Build binaries
WORKDIR /app

COPY . .

RUN cargo build --release --features gpu

FROM nvidia/cuda:12.9.1-runtime-ubuntu24.04 AS runtime

# Taken from https://0xpolygonhermez.github.io/zisk/getting_started/installation.html
RUN apt-get update && apt-get install -y --no-install-recommends \
    xz-utils \
    jq \
    curl \
    build-essential \
    qemu-system \
    libomp-dev \
    libgmp-dev \
    nlohmann-json3-dev \
    protobuf-compiler \
    uuid-dev \
    libgrpc++-dev \
    libsecp256k1-dev \
    libsodium-dev \
    libpqxx-dev \
    nasm \
    libopenmpi-dev \
    openmpi-bin \
    openmpi-common \
    libclang-dev \
    clang \
    gcc-riscv64-unknown-elf \
    libprotobuf-dev \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --no-modify-path

# Install proving keys
WORKDIR /app

COPY --chmod=+x ./ziskup/install.sh install.sh

RUN SETUP_KEY=proving ./install.sh && \
    rm install.sh && \
    rm -r "/root/.zisk/toolchains" && \
    find "/root/.zisk/provingKey" -name "*.consttree" -type f -delete

# Copy binaries
COPY --from=builder /app/target/release/cargo-zisk /root/.zisk/bin/cargo-zisk
COPY --from=builder /app/target/release/libzisk_witness.so /root/.zisk/bin/libzisk_witness.so
COPY --from=builder /app/target/release/zisk-coordinator /root/.zisk/bin/zisk-coordinator
COPY --from=builder /app/target/release/zisk-worker /root/.zisk/bin/zisk-worker

ENV PATH=/root/.zisk/bin:$PATH

# Copy configs
COPY ./distributed/crates/coordinator/config/ /app/config/coordinator/
COPY ./distributed/crates/worker/config/ /app/config/worker/

CMD ["/bin/bash"]
