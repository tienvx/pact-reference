ARG ARCH=
FROM --platform=$BUILDPLATFORM rust:1-alpine3.21 AS builder
ARG BIN_ARCH=x86_64
ARG VERSION=1.1.1

RUN wget -O pact_verifier_cli.gz https://github.com/pact-foundation/pact-reference/releases/download/pact_verifier_cli-v${VERSION}/pact_verifier_cli-linux-${BIN_ARCH}.gz
RUN gunzip pact_verifier_cli.gz
RUN chmod +x pact_verifier_cli

# Now, we need to build our _real_ Docker container, copying in the executable.
FROM --platform=$BUILDPLATFORM alpine:3.21
RUN apk --no-cache add ca-certificates
COPY --from=builder \
    /pact_verifier_cli \
    /usr/local/bin/

ENTRYPOINT ["/usr/local/bin/pact_verifier_cli"]
