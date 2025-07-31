FROM --platform=linux/amd64 ubuntu:22.04

# Install necessary tools
RUN apt-get update && apt-get install -y \
    curl \
    wget \
    ca-certificates \
    man-db \
    && rm -rf /var/lib/apt/lists/*

# Remove dpkg exclusions that prevent man pages from being installed
RUN rm -f /etc/dpkg/dpkg.cfg.d/excludes

# Create a directory for testing
WORKDIR /test

# Copy the verification script
COPY verify-deb.sh /test/verify-deb.sh
RUN chmod +x /test/verify-deb.sh

# Run the verification script
ENTRYPOINT ["/test/verify-deb.sh"]