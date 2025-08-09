# Build stage for Go tools
FROM golang:1.23 AS go-tools
ENV CGO_ENABLED=0
RUN go install github.com/golangci/golangci-lint/cmd/golangci-lint@v1.57.2 && \
    go install gotest.tools/gotestsum@latest && \
    go install github.com/goreleaser/goreleaser@v1.26.2

# Main stage
FROM ubuntu:24.04

# Prevent interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive
ENV TZ=UTC

# Install system dependencies from deps.txt
COPY deps.txt /tmp/deps.txt
RUN apt-get update && \
    apt-get install -y $(grep -v '^#' /tmp/deps.txt | grep -v '^$') && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Configure locale
RUN locale-gen en_US.UTF-8
ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US:en
ENV LC_ALL=en_US.UTF-8

# Create development user FIRST (before any other user creation)
ARG USERNAME=developer
ARG USER_UID=1000
ARG USER_GID=$USER_UID

# Create the developer user with the specified UID/GID
RUN groupadd --gid $USER_GID $USERNAME || true && \
    useradd --uid $USER_UID --gid $USER_GID -m $USERNAME -s /usr/bin/zsh && \
    echo "$USERNAME ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Install Node.js 20.x (newer version for better compatibility)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Install global npm packages
RUN npm install -g @google/gemini-cli && \
    npm install -g @anthropic-ai/claude-code

# Install Go (detect architecture)
ENV GO_VERSION=1.23.5
RUN ARCH=$(dpkg --print-architecture) && \
    curl -fsSL https://go.dev/dl/go${GO_VERSION}.linux-${ARCH}.tar.gz | tar -C /usr/local -xz
ENV PATH="/usr/local/go/bin:${PATH}"
ENV CGO_ENABLED=0

# Create Go directories with proper permissions
RUN mkdir -p /opt/go/bin /opt/go/pkg /opt/go/cache && \
    chmod -R 777 /opt/go
ENV GOPATH="/opt/go"
ENV GOCACHE="/opt/go/cache"
ENV PATH="${GOPATH}/bin:${PATH}"

# Copy Go tools from build stage
COPY --from=go-tools /go/bin/* /opt/go/bin/

# Install Homebrew for the developer user (avoid UID conflicts)
# Use a different UID for linuxbrew if developer is using 1001
RUN BREW_UID=2001 && \
    if [ "$USER_UID" = "1001" ]; then BREW_UID=2001; else BREW_UID=1001; fi && \
    useradd -m -s /bin/bash -u $BREW_UID linuxbrew && \
    echo 'linuxbrew ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers && \
    su - linuxbrew -c '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"' && \
    chmod -R 755 /home/linuxbrew/.linuxbrew && \
    chown -R linuxbrew:linuxbrew /home/linuxbrew/.linuxbrew && \
    mkdir -p /home/linuxbrew/.linuxbrew/var/homebrew/linked && \
    chown -R linuxbrew:linuxbrew /home/linuxbrew/.linuxbrew/var/homebrew/linked && \
    chmod -R g+w /home/linuxbrew/.linuxbrew && \
    usermod -aG linuxbrew $USERNAME
    
ENV PATH="/home/linuxbrew/.linuxbrew/bin:${PATH}"
ENV HOMEBREW_PREFIX="/home/linuxbrew/.linuxbrew"
ENV HOMEBREW_CELLAR="/home/linuxbrew/.linuxbrew/Cellar"
ENV HOMEBREW_REPOSITORY="/home/linuxbrew/.linuxbrew/Homebrew"

# Copy git configuration (without user section)
COPY .gitconfig /home/$USERNAME/.gitconfig

# Setup zsh with oh-my-zsh
USER $USERNAME
RUN sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended && \
    echo 'export PATH="/usr/local/go/bin:/opt/go/bin:$PATH"' >> ~/.zshrc && \
    echo 'export GOPATH=/opt/go' >> ~/.zshrc && \
    echo 'export GOCACHE=/opt/go/cache' >> ~/.zshrc && \
    echo 'eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"' >> ~/.zshrc && \
    echo 'eval "$(direnv hook zsh)"' >> ~/.zshrc && \
    echo '# Source login shell script if it exists' >> ~/.zshrc && \
    echo '[ -f /container-scripts/login-shell.sh ] && source /container-scripts/login-shell.sh' >> ~/.zshrc

# Set working directory
WORKDIR /workspace

# Switch to zsh shell
SHELL ["/usr/bin/zsh", "-c"]
CMD ["/usr/bin/zsh"]