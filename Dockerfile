FROM rust:1.75.0

# Update and install necessary packages
RUN apt-get update && \
    apt-get install -y clang cmake libsnappy-dev

# Add a new user and set the working directory
RUN adduser --disabled-login --system --shell /bin/false --uid 1000 user
WORKDIR /home/user

# Copy the current directory contents into the container
COPY . .

# Build the application
RUN cargo build --release
RUN mv /home/user/target/release/addrindexrs /usr/local/bin/

ENTRYPOINT [ "addrindexrs" ]

# Default command
CMD [ "-vvv" ]
