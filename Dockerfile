ARG BASE_IMAGE=openjdk:17-slim

FROM ${BASE_IMAGE} AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl net-tools lsof netcat procps less \
    && rm -rf /var/lib/apt/lists/*

COPY bifrore-*-standalone.tar.gz /
RUN mkdir /bifrore && tar -zxvf /bifrore-*-standalone.tar.gz --strip-components 1 -C /bifrore \
    && rm -rf /bifrore-*-standalone.tar.gz

FROM ${BASE_IMAGE}

RUN groupadd -r -g 1000 bifrore \
    && useradd -r -m -u 1000 -g bifrore bifrore \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        net-tools lsof netcat procps less \
    && rm -rf /var/lib/apt/lists/*

COPY --chown=bifrore:bifrore --from=builder /bifrore /home/bifrore/

ENV JAVA_HOME=/usr/local/openjdk-17 \
    PATH="/usr/local/openjdk-17/bin:$PATH"

WORKDIR /home/bifrore

USER bifrore

RUN echo "alias ll='ls -al'" >> ~/.bashrc


CMD ["./bin/standalone.sh", "start", "-fg"]

