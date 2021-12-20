
FROM paritytech/ci-linux:production as builder

RUN rm -rf /usr/local/rustup/toolchains/
RUN rustup default stable
RUN rustup update nightly
RUN rustup target add wasm32-unknown-unknown --toolchain nightly

LABEL maintainer="pendulum-dev"
ARG PROFILE=release
ARG GIT_REPO="https://github.com/pendulum-chain/pendulum.git"
ARG GIT_BRANCH="master"
ARG GIT_CLONE_DEPTH="--depth 1"
WORKDIR /builds/

#Build pendulum
RUN git clone --recursive ${GIT_REPO} ${GIT_CLONE_DEPTH}

WORKDIR /builds/pendulum
RUN git checkout ${GIT_BRANCH}
RUN cargo build --${PROFILE}
COPY ./target/${PROFILE}/parachain-collator /pendulum-collator

FROM ubuntu
ENV DEBIAN_FRONTEND noninteractive
RUN apt update && apt install git gnupg2 curl ca-certificates vim npm nodejs wget awscli -y
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

ARG POLKADOT_LAUNCH_REPO="https://github.com/paritytech/polkadot-launch"
RUN git clone --recursive ${POLKADOT_LAUNCH_REPO} ${GIT_CLONE_DEPTH}
WORKDIR /builds/polkadot-launch
RUN yarn global add polkadot-launch

WORKDIR /
COPY --from=builder /pendulum-collator /

EXPOSE 30333 9933 9944