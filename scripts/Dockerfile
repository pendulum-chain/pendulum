# build pendulum
FROM parity/polkadot:latest as builder

LABEL maintainer="pendulum-dev"
ARG PROFILE=release
ARG GIT_REPO="https://github.com/pendulum-chain/pendulum.git"
ARG GIT_BRANCH="main"
ARG GIT_CLONE_DEPTH="--depth 1"
WORKDIR /builds/

#Build pendulum
RUN apt update && apt install git
RUN git clone --recursive ${GIT_REPO} ${GIT_CLONE_DEPTH}

WORKDIR /builds/pendulum
RUN git checkout ${GIT_BRANCH}
RUN cargo build --${PROFILE}
RUN cp target/${PROFILE}/pendulum /pendulum

FROM ubuntu
ENV DEBIAN_FRONTEND noninteractive
RUN apt update && apt install gnupg2 curl ca-certificates vim npm nodejs wget awscli -y
RUN curl -sS https://dl.yarnpkg.com/debian/pubkey.gpg | apt-key add - && echo "deb https://dl.yarnpkg.com/debian/ stable main" | tee /etc/apt/sources.list.d/yarn.list

WORKDIR /builds/
RUN apt update && apt install yarn -y
RUN apt purge nodejs npm -y

RUN apt-get install -y \
    software-properties-common \
    npm
RUN npm install npm@latest -g && \
    npm install n -g && \
    n stable

ARG POLKADOT_LAUNCH_REPO="https://github.com/polkadot-launch.git"
RUN git clone --recursive ${POLKADOT_LAUNCH_REPO} ${GIT_CLONE_DEPTH}
WORKDIR /builds/polkadot-launch
RUN yarn global add polkadot-launch

WORKDIR /
COPY --from=builder /pendulum /
COPY --from=builder /polkadot /
COPY --from=builder /subkey /

EXPOSE 30333 9933 9944