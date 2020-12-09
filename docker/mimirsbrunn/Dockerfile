ARG RUST_VERSION
ARG DEBIAN_VERSION

FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} as builder

WORKDIR /srv/mimirsbrunn

ENV DEBIAN_FRONTEND noninteractive

ARG DEBIAN_VERSION

RUN if [ "${DEBIAN_VERSION}" = "buster" ]; then \
  apt-get update \
    && apt-get install -y make libssl-dev git \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*; \
elif [ "${DEBIAN_VERSION}" = "stretch" ]; then \
  apt-get update \
    && apt-get install -y make libssl1.0-dev git \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*; \
else \
  echo "Unsupported debian version '$DEBIAN_VERSION'"; \
fi

COPY . ./

RUN rm Cargo.lock

RUN cargo build --release --features db-storage

FROM debian:${DEBIAN_VERSION}-slim

WORKDIR /srv

ENV DEBIAN_FRONTEND noninteractive

ARG DEBIAN_VERSION

RUN if [ "${DEBIAN_VERSION}" = "buster" ]; then \
  apt-get update \
    && apt-get install -y libcurl4 sqlite3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*; \
elif [ "${DEBIAN_VERSION}" = "stretch" ]; then \
  apt-get update \
    && apt-get install -y libcurl3 sqlite3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*; \
else \
  echo "Unsupported debian version '$DEBIAN_VERSION'"; \
fi

RUN mkdir /etc/mimirsbrunn

COPY --from=builder /srv/mimirsbrunn/target/release/osm2mimir /usr/bin/osm2mimir
COPY --from=builder /srv/mimirsbrunn/target/release/cosmogony2mimir /usr/bin/cosmogony2mimir
COPY --from=builder /srv/mimirsbrunn/target/release/bano2mimir /usr/bin/bano2mimir
COPY --from=builder /srv/mimirsbrunn/target/release/openaddresses2mimir /usr/bin/openaddresses2mimir
COPY --from=builder /srv/mimirsbrunn/config/* /etc/mimirsbrunn/
