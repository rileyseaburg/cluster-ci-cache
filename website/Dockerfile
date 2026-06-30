FROM rust:1.86-slim

WORKDIR /app

RUN cargo install tetherscript --version 0.1.0-alpha.12

COPY server.tether ./server.tether

EXPOSE 8787

CMD ["tetherscript", "run", "--grant-fs", ".", "server.tether"]
