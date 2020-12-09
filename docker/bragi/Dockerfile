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

RUN cargo build --release

ARG DEBIAN_VERSION

FROM debian:${DEBIAN_VERSION}-slim

WORKDIR /srv

ENV DEBIAN_FRONTEND noninteractive

ARG DEBIAN_VERSION

RUN if [ "${DEBIAN_VERSION}" = "buster" ]; then \
  apt-get update \
    && apt-get install -y libcurl4 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*; \
elif [ "${DEBIAN_VERSION}" = "stretch" ]; then \
  apt-get update \
    && apt-get install -y libcurl3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*; \
else \
  echo "Unsupported debian version '$DEBIAN_VERSION'"; \
fi

COPY --from=builder /srv/mimirsbrunn/target/release/bragi /srv/bragi

EXPOSE 4000
ENV BRAGI_ES http://localhost:9200/munin
ENV RUST_LOG=info,hyper=info

CMD ["/srv/bragi", "-b", "0.0.0.0:4000"]
