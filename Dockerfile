# syntax=docker/dockerfile:1.7
FROM rust:1.85 AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p threefs-s3-gateway

FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=build /src/target/release/threefs-s3-gateway /app/threefs-s3-gateway
USER nonroot
ENV RUST_LOG=info
EXPOSE 9000
ENTRYPOINT ["/app/threefs-s3-gateway"]

