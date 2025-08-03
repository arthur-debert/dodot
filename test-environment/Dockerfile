# Use Ubuntu LTS as base
FROM ubuntu:22.04

# Avoid interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Install essential packages and Go
RUN apt-get update && apt-get install -y \
    curl \
    git \
    wget \
    sudo \
    zsh \
    vim \
    tmux \
    build-essential \
    locales \
    && rm -rf /var/lib/apt/lists/*

# Install Go 1.23 (matching project requirements)
# IMPORTANT: Installing Go in the image layer for caching, not mounting
# Detect architecture and download appropriate Go binary
RUN ARCH=$(dpkg --print-architecture) && \
    case ${ARCH} in \
        amd64) GOARCH="amd64" ;; \
        arm64) GOARCH="arm64" ;; \
        *) echo "Unsupported architecture: ${ARCH}" && exit 1 ;; \
    esac && \
    wget -q https://go.dev/dl/go1.23.0.linux-${GOARCH}.tar.gz && \
    tar -C /usr/local -xzf go1.23.0.linux-${GOARCH}.tar.gz && \
    rm go1.23.0.linux-${GOARCH}.tar.gz

# Install ShellSpec for shell testing
RUN git clone --depth 1 https://github.com/shellspec/shellspec.git /tmp/shellspec && \
    cd /tmp/shellspec && \
    sudo make install PREFIX=/usr/local && \
    rm -rf /tmp/shellspec

# Generate locale
RUN locale-gen en_US.UTF-8
ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US:en
ENV LC_ALL=en_US.UTF-8

# Create test user with sudo privileges
RUN useradd -m -s /bin/zsh testuser && \
    echo "testuser ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Switch to test user
USER testuser
WORKDIR /home/testuser

# Copy setup scripts
COPY --chown=testuser:testuser scripts/ /setup-scripts/

# Copy sample dotfiles
COPY --chown=testuser:testuser sample-dotfiles/ /dotfiles/

# Run Homebrew setup
RUN /setup-scripts/setup-brew.sh

# Set DOTFILES_ROOT
ENV DOTFILES_ROOT=/dotfiles

# Add Homebrew and Go to PATH for all shells
ENV PATH="/home/linuxbrew/.linuxbrew/bin:/usr/local/go/bin:${PATH}"

# Make test scripts easily accessible
RUN echo 'export PATH="/scripts:$PATH"' >> ~/.zshrc

# Default to zsh shell
CMD ["/bin/zsh"]