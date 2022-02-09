FROM paritytech/ci-linux:production as builder

LABEL maintainer="pendulum-dev"
ARG GIT_CLONE_DEPTH=1
ARG PENDULUM_LAUNCH_REPO="https://github.com/pendulum-chain/pendulum-launch"
# ARG POLKADOT_LAUNCH_REPO="https://github.com/paritytech/polkadot-launch"

# Build penulum-collator

WORKDIR /src/pendulum
COPY ./ .
RUN cargo build --release
RUN strip ./target/release/parachain-collator

# Install pendulum-launch
# WORKDIR /src
# RUN git clone --recursive ${PENDULUM_LAUNCH_REPO} ${GIT_CLONE_DEPTH}
# WORKDIR /src/penulum-launch
# RUN cargo build --${PROFILE}
# RUN strip ./target/${PROFILE}/parachain-collator

# Move binaries into fresh box
FROM ubuntu
ENV DEBIAN_FRONTEND noninteractive
RUN apt-get -y update && apt-get -y install git gnupg2 curl ca-certificates wget awscli -y
# RUN curl -sS https://dl.yarnpkg.com/debian/pubkey.gpg | apt-key add - && echo "deb https://dl.yarnpkg.com/debian/ stable main" | tee /etc/apt/sources.list.d/yarn.list

WORKDIR /
COPY --from=builder /src/pendulum/target/release/parachain-collator /bin/pendulum-collator
# COPY --from=builder /src/pendulum/.config/launch.json /launch.json
# COPY --from=builder /src/pendulum-launch/target/{PROFILE}/pendulum-launch /bin/pendulum-launch
RUN wget https://raw.githubusercontent.com/pendulum-chain/pendulum/master/specs/live-chachacha-spec.json 
RUN wget https://raw.githubusercontent.com/centrifuge/polkadot/chachacha/node/service/res/rococo-chachacha.json

EXPOSE 30333 9933 9944
ENTRYPOINT [                    \ 
    "pendulum-collator",        \ 
    "--collator",               \ 
    "--allow-private-ipv4",     \ 
    "--unsafe-ws-external",     \
    "--rpc-cors",               \
    "all",                      \
    "--rpc-external",           \ 
    "--rpc-methods",            \
    "Unsafe",                   \
    "--name",                   \
    "pendulum-collator-fox-1",  \
    "--ws-port",                \
    "9945",                     \
    "--port",                   \ 
    "30335",                    \
    "--rpc-port",               \ 
    "9935",                     \
    "--chain",                  \ 
    "live-chachacha-spec.json", \ 
    "--execution=Native",       \
    "--",                       \
    "--port",                   \ 
    "30334",                    \
    "--chain",                  \
    "rococo-chachacha.json",    \ 
    "--execution=wasm",         \
    "--sync",                   \
    "fast",                     \
    "--pruning",                \ 
    "archive",                  \ 
]
# ENTRYPOINT [ "pendulum-launch", "--config", "/launch.json", "--log", "/tmp/pendulum" ]
