# syntax=docker/dockerfile:1
# One Dockerfile for every image: APP picks the crate to build (helpdesk,
# research or mcp-server).
#
#   docker build --build-arg APP=helpdesk --build-arg VERSION=0.1.0 -t helpdesk .
#
# A fully static (musl) binary in a distroless "static" image: no shell, no
# libc, a nonroot user and the binary. VERSION is the image tag; the program
# shows it. The listen address comes from the Deployment.
FROM rust:1.99-alpine AS build
ARG APP
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY agents agents
COPY mcp-server mcp-server
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p "$APP" && cp "target/release/$APP" /app

FROM gcr.io/distroless/static-debian12:nonroot
ARG VERSION=dev-build
COPY --from=build /app /app
ENV APP_VERSION=$VERSION
ENTRYPOINT ["/app"]
