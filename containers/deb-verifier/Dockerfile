FROM --platform=linux/amd64 ubuntu:22.04

# Install necessary tools
RUN apt-get update && apt-get install -y \
    curl \
    wget \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a directory for testing
WORKDIR /test

# Copy the verification script
COPY verify-deb.sh /test/verify-deb.sh
RUN chmod +x /test/verify-deb.sh

# Run the verification script
ENTRYPOINT ["/test/verify-deb.sh"]