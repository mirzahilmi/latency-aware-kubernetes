FROM golang:1.25.0-bookworm AS builder
WORKDIR /src

RUN \
    --mount=type=cache,target=/go/pkg/mod \
    --mount=type=bind,target=. \
    go mod download

RUN \
    --mount=type=cache,target=/go/pkg/mod \
    --mount=type=bind,target=. \
    CGO_ENABLED=0 go build -ldflags "-s -w" -o /artifact/scheduler ./cmd/scheduler/ && \
    CGO_ENABLED=0 go build -ldflags "-s -w" -o /artifact/descheduler ./cmd/descheduler/
FROM cgr.dev/chainguard/wolfi-base:latest
WORKDIR /app

COPY --from=builder --chown=nonroot:nonroot /artifact/scheduler /app/scheduler
COPY --from=builder --chown=nonroot:nonroot /artifact/descheduler /app/descheduler
USER nonroot

# default scheduler
ENTRYPOINT ["/app/scheduler"]
