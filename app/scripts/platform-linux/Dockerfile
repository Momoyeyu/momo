ARG BASE_IMAGE=ubuntu:24.04
FROM ${BASE_IMAGE}
USER root
ENTRYPOINT []
ENV DEBIAN_FRONTEND=noninteractive
RUN if [ -f /etc/apt/sources.list.d/ubuntu.sources ]; then sed -i 's|http://archive.ubuntu.com/ubuntu/|https://mirrors.ustc.edu.cn/ubuntu/|g; s|http://security.ubuntu.com/ubuntu/|https://mirrors.ustc.edu.cn/ubuntu/|g' /etc/apt/sources.list.d/ubuntu.sources; fi && apt-get update && apt-get install -y --no-install-recommends ca-certificates curl build-essential file pkg-config libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf libxdo-dev xvfb xauth dbus-x11 libgl1-mesa-dri libegl1 python3 python3-venv python3-dev && rm -rf /var/lib/apt/lists/*
ENV PATH=/tools/node/bin:/root/.cargo/bin:/tools/bin:$PATH
ENV RUSTUP_DIST_SERVER=https://rsproxy.cn RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup
WORKDIR /workspace
