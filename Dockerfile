FROM rust:1-bullseye AS build-image

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        wget \
        curl \
        libpq-dev \
        pkg-config \
        libssl-dev \
        clang \
        build-essential \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates

COPY . /graphcast-3la
WORKDIR /graphcast-3la

RUN sh install-golang.sh
ENV DATABASE_URL=postgres://postgres:postgres@localhost:5432/test_3la
RUN echo "DATABASE_URL=postgres://postgres:postgres@localhost:5432/test_3la" > .env
ENV PATH=$PATH:/usr/local/go/bin

RUN cargo build --release

FROM alpine:3.17.3 as alpine
RUN set -x \
    && apk update \
    && apk add --no-cache upx dumb-init
COPY --from=build-image /graphcast-3la/target/release/graphcast-3la /graphcast-3la/target/release/graphcast-3la
RUN upx --overlay=strip --best /graphcast-3la/target/release/graphcast-3la

FROM gcr.io/distroless/cc AS runtime
COPY --from=build-image /usr/share/zoneinfo /usr/share/zoneinfo
COPY --from=build-image /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=build-image /etc/passwd /etc/passwd
COPY --from=build-image /etc/group /etc/group
COPY --from=alpine /usr/bin/dumb-init /usr/bin/dumb-init
COPY --from=alpine "/graphcast-3la/target/release/graphcast-3la" "/usr/local/bin/graphcast-3la"
ENTRYPOINT [ "/usr/bin/dumb-init", "--", "/usr/local/bin/graphcast-3la" ]
