FROM rust:1.85-bookworm AS build
WORKDIR /src
COPY v2 /src/v2
RUN cargo build --manifest-path /src/v2/Cargo.toml --release -p sel-api -p sel-cli

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates libssl3 wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home-dir /nonexistent --shell /usr/sbin/nologin selapi
COPY --from=build /src/v2/target/release/sel-api /usr/local/bin/sel-api
COPY --from=build /src/v2/target/release/sel-cli /usr/local/bin/sel-cli
USER selapi
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/sel-api"]
