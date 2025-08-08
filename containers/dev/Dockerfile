# Build stage for Go tools
FROM golang:1.24.5 AS go-tools
ENV CGO_ENABLED=0
RUN go install github.com/golangci/golangci-lint/cmd/golangci-lint@v1.57.2 && \
    go install gotest.tools/gotestsum@latest && \
    go install github.com/goreleaser/goreleaser/v2@latest

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
    rm -rf /var/lib/apt/lists/* /tmp/deps.txt

# Configure locale
RUN locale-gen en_US.UTF-8
ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US:en
ENV LC_ALL=en_US.UTF-8

# Install Go (detect architecture)
ENV GO_VERSION=1.24.5
RUN ARCH=$(dpkg --print-architecture) && \
    curl -fsSL https://go.dev/dl/go${GO_VERSION}.linux-${ARCH}.tar.gz | tar -C /usr/local -xz
ENV PATH="/usr/local/go/bin:${PATH}"
ENV GOPATH="/go"
ENV PATH="${GOPATH}/bin:${PATH}"
ENV CGO_ENABLED=0

# Copy Go tools from build stage
COPY --from=go-tools /go/bin/* /go/bin/

# Install Homebrew
RUN useradd -m -s /bin/bash linuxbrew && \
    echo 'linuxbrew ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers && \
    su - linuxbrew -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
ENV PATH="/home/linuxbrew/.linuxbrew/bin:${PATH}"
ENV HOMEBREW_PREFIX="/home/linuxbrew/.linuxbrew"
ENV HOMEBREW_CELLAR="/home/linuxbrew/.linuxbrew/Cellar"
ENV HOMEBREW_REPOSITORY="/home/linuxbrew/.linuxbrew/Homebrew"

# Create development user
ARG USERNAME=developer
ARG USER_UID=1000
ARG USER_GID=$USER_UID

RUN (groupadd --gid $USER_GID $USERNAME || true) && \
    useradd --uid $USER_UID --gid $USER_GID -m $USERNAME -s /usr/bin/zsh && \
    echo "$USERNAME ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Copy git configuration (without user section)
COPY .gitconfig /home/$USERNAME/.gitconfig

# Setup zsh with oh-my-zsh
USER $USERNAME
RUN sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended && \
    echo 'export PATH="/usr/local/go/bin:$GOPATH/bin:$PATH"' >> ~/.zshrc && \
    echo 'export PATH="/home/linuxbrew/.linuxbrew/bin:$PATH"' >> ~/.zshrc && \
    echo 'eval "$(direnv hook zsh)"' >> ~/.zshrc && \
    echo '# Source login shell script if it exists' >> ~/.zshrc && \
    echo '[ -f /container-scripts/login-shell.sh ] && source /container-scripts/login-shell.sh' >> ~/.zshrc && \
    mkdir -p ~/.cache/go-build && \
    chmod -R 755 ~/.cache

# Set working directory
WORKDIR /workspace

# Switch to zsh shell
SHELL ["/usr/bin/zsh", "-c"]
CMD ["/usr/bin/zsh"]