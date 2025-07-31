# Use Ubuntu LTS as base
FROM ubuntu:22.04

# Avoid interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Install essential packages
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

# Set DOTFILES_ROOT
ENV DOTFILES_ROOT=/dotfiles

# Default to zsh shell
CMD ["/bin/zsh"]