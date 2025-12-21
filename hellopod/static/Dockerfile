FROM golang:1.25.2-bookworm AS builder
WORKDIR /src

COPY ./go.mod ./go.sum ./
RUN \
    --mount=type=cache,target=/go/pkg/mod \
    go mod download

COPY . .
RUN \
    --mount=type=cache,target=/go/pkg/mod \
    go build -ldflags "-s -w" -mod=readonly -o=/artifact/program .

FROM cgr.dev/chainguard/wolfi-base:latest
WORKDIR /bin

COPY --from=builder --chown=nonroot:nonroot /artifact/program /bin
RUN apk add dumb-init

USER nonroot

# see https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry#labelling-container-images
LABEL "org.opencontainers.image.source"="https://github.com/mirzahilmi/latency-aware-kubernetes"
LABEL "org.opencontainers.image.description"="🔒 Secured LDR & PIR in transit sensor readings"
LABEL "org.opencontainers.image.licenses"="AGPL-3.0"

# see https://github.com/gofiber/fiber/issues/1036#issuecomment-841763449
ENTRYPOINT ["/usr/bin/dumb-init", "--"]
CMD ["/bin/program"]
