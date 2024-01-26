FROM rust:1.75.0

RUN apt-get update
RUN apt-get install -y clang cmake
RUN apt-get install -y libsnappy-dev

RUN adduser --disabled-login --system --shell /bin/false --uid 1000 user
WORKDIR /home/user
COPY ./ /home/user 

RUN cargo check
RUN cargo build --release

COPY addrindexrs.toml /home/user

ENTRYPOINT [ "cargo", "run", "--release", "--" ]
CMD [ "-vvv" ]
