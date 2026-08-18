ARG ALT_IMAGE=alt:latest

FROM ${ALT_IMAGE} AS rust-deps
WORKDIR /src
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
RUN apt-get update && \
    apt-get install -y curl ca-certificates pkg-config gcc glibc-devel make openssl-devel libkrb5-devel clang clang-devel && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable && \
    rustc --version && cargo --version && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

# Dependency layer: copy manifests only, create a temporary stub binary and compile deps.
COPY backend/Cargo.toml backend/Cargo.lock backend/
RUN mkdir -p backend/src && \
    printf 'fn main() { println!("dependency build stub"); }\n' > backend/src/main.rs && \
    cargo build --release --manifest-path backend/Cargo.toml --bin backend && \
    rm -rf backend/src && \
    rm -f backend/target/release/backend backend/target/release/deps/backend-* && \
    find backend/target/release/.fingerprint -maxdepth 1 -name 'backend-*' -exec rm -rf {} +

FROM rust-deps AS rust-build
WORKDIR /src
COPY backend backend
COPY README.md ./
# Build real project. Remove own-package fingerprints to avoid Cargo considering the stub fresh.
RUN rm -f backend/target/release/backend backend/target/release/deps/backend-* && \
    find backend/target/release/.fingerprint -maxdepth 1 -name 'backend-*' -exec rm -rf {} + && \
    touch backend/src/main.rs && \
    cargo build --release --manifest-path backend/Cargo.toml --bin backend

FROM ${ALT_IMAGE} AS frontend-build
WORKDIR /src/frontend
RUN apt-get update && \
    apt-get install -y node npm ca-certificates glibc-core glibc-pthread && \
    apt-get clean && rm -rf /var/lib/apt/lists/*
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/.env ./
COPY frontend ./
RUN npm run build

FROM ${ALT_IMAGE} AS runtime
WORKDIR /app
RUN apt-get update && \
    apt-get install -y ca-certificates libkrb5 libldap openldap-clients tzdata && \
    apt-get clean && rm -rf /var/lib/apt/lists/* && \
    mkdir -p /app/frontend/dist /app/backend /app/output /app/.kerberos-cache /secrets

COPY --from=rust-build /src/backend/target/release/backend /usr/local/bin/sgu-priemka
COPY --from=rust-build /src/backend/templates /app/backend/templates
COPY --from=rust-build /src/backend/groups.toml /app/groups.toml
COPY --from=frontend-build /src/frontend/dist /app/frontend/dist
COPY backend/.env /app/.env

VOLUME ["/secrets", "/app/output", "/app/.kerberos-cache"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/sgu-priemka"]
