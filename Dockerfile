FROM rust:1-slim-trixie AS builder

ENV RUSTFLAGS="-C link-arg=-s"

WORKDIR /app
COPY . .

# --locked: build exactly the dependency versions recorded in Cargo.lock.
RUN cargo build --release --locked

# --- Final Stage ---
# The :nonroot variant runs as an unprivileged user (uid 65532) instead of root.
FROM gcr.io/distroless/cc-debian13:nonroot
WORKDIR /app

COPY --from=builder /app/target/release/ipcheck /app/ipcheck

USER 65532:65532
EXPOSE 5000
CMD ["./ipcheck"]
