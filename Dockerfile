# syntax=docker/dockerfile:1
# A fully static (musl) binary in a distroless "static" image: no shell, no
# libc, a nonroot user and the binary. VERSION is the image tag; the agent
# shows it.
FROM rust:1.99-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked && cp target/release/agent /agent

FROM gcr.io/distroless/static-debian12:nonroot
ARG VERSION=dev-build
COPY --from=build /agent /agent
ENV AGENT_ADDR=0.0.0.0:3000 APP_VERSION=$VERSION
EXPOSE 3000
ENTRYPOINT ["/agent"]
