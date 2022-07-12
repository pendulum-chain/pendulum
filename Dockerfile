FROM ubuntu:20.04

ARG PROFILE=release
ARG BINARY=pendulum-parachain

ENV DEBIAN_FRONTEND=noninteractive

# install tools and dependencies
RUN apt-get update && apt-get upgrade -y && \
      apt-get install -y libssl1.1 ca-certificates curl tini && \
      apt-get autoremove -y &&  apt-get clean && \
      find /var/lib/apt/lists/ -type f -not -name lock -delete

COPY target/${PROFILE}/pendulum-parachain /usr/local/bin

# Checks
RUN chmod +x /usr/local/bin/pendulum-parachain && \
    ldd /usr/local/bin/pendulum-parachain && \
    /usr/local/bin/pendulum-parachain --version

EXPOSE 30333 9933 9944
VOLUME ["/data"]

ENTRYPOINT ["tini", "--", "/usr/local/bin/pendulum-parachain"]