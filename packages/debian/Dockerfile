FROM debian:jessie as builder

WORKDIR /home/pkg

COPY buster mimirsbrunn

FROM localhost:5000/mimirsbrunn:1.19.0-rc11 as mimirsbrunn

FROM debian:jessie-slim

WORKDIR /srv

COPY --from=builder /home/pkg/mimirsbrunn /srv/mimirsbrunn
COPY --from=mimirsbrunn /usr/bin/osm2mimir /srv/mimirsbrunn/usr/bin/osm2mimir
COPY --from=mimirsbrunn /usr/bin/cosmogony2mimir /srv/mimirsbrunn/usr/bin/cosmogony2mimir
COPY --from=mimirsbrunn /usr/bin/bano2mimir /srv/mimirsbrunn/usr/bin/bano2mimir
COPY --from=mimirsbrunn /usr/bin/openaddresses2mimir /srv/mimirsbrunn/usr/bin/openaddresses2mimir

RUN dpkg-deb --build mimirsbrunn

CMD /bin/bash
