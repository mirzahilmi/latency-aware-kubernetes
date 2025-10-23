FROM golang:1.25.0-bookworm AS builder
WORKDIR /src

RUN \
    --mount=type=cache,target=/go/pkg/mod \
    --mount=type=bind,target=. \
    go mod download

RUN \
    --mount=type=cache,target=/go/pkg/mod \
    --mount=type=bind,target=. \
    CGO_ENABLED=0 go build -ldflags "-s -w" -o /artifact/program ./cmd/scheduler/

FROM cgr.dev/chainguard/wolfi-base:latest
WORKDIR /bin

COPY --from=builder --chown=nonroot:nonroot /artifact/program /bin
USER nonroot

ENTRYPOINT ["/bin/program"]
