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

COPY . /listener-radio
WORKDIR /listener-radio

RUN sh install-golang.sh
ARG SQLX_OFFLINE=true
ARG DATABASE_URL=postgresql://postgres:password@localhost:5432/postgres
RUN echo "DATABASE_URL=postgresql://postgres:password@localhost:5432/postgres" > .env
RUN ls
ENV PATH=$PATH:/usr/local/go/bin

RUN cargo build --release

FROM alpine:3.17.3 as alpine
RUN set -x \
    && apk update \
    && apk add --no-cache upx dumb-init
COPY --from=build-image /listener-radio/target/release/listener-radio /listener-radio/target/release/listener-radio
RUN upx --overlay=strip --best /listener-radio/target/release/listener-radio

FROM gcr.io/distroless/cc AS runtime
COPY --from=build-image /usr/share/zoneinfo /usr/share/zoneinfo
COPY --from=build-image /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=build-image /etc/passwd /etc/passwd
COPY --from=build-image /etc/group /etc/group
COPY --from=alpine /usr/bin/dumb-init /usr/bin/dumb-init
COPY --from=alpine "/listener-radio/target/release/listener-radio" "/usr/local/bin/listener-radio"
ENTRYPOINT [ "/usr/bin/dumb-init", "--", "/usr/local/bin/listener-radio" ]
