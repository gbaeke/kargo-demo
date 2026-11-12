# syntax=docker/dockerfile:1
# One Dockerfile for every agent: AGENT picks the crate to build.
#
#   docker build --build-arg AGENT=helpdesk --build-arg VERSION=0.1.0 -t helpdesk .
#
# A fully static (musl) binary in a distroless "static" image: no shell, no
# libc, a nonroot user and the binary. VERSION is the image tag; the agent
# shows it.
FROM rust:1.99-alpine AS build
ARG AGENT
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY agents agents
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p "$AGENT" && cp "target/release/$AGENT" /agent

FROM gcr.io/distroless/static-debian12:nonroot
ARG VERSION=dev-build
COPY --from=build /agent /agent
ENV AGENT_ADDR=0.0.0.0:3000 APP_VERSION=$VERSION
EXPOSE 3000
ENTRYPOINT ["/agent"]
