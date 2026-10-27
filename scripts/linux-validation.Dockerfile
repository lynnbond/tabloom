# Validation image only. Does not package or publish Tabloom.
FROM public.ecr.aws/docker/library/rust:bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libfontconfig1-dev libfreetype6-dev libxkbcommon-dev \
    libwayland-dev libx11-dev libxi-dev libxcursor-dev libxrandr-dev \
    libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libegl1-mesa-dev \
    libgl1-mesa-dev libssl-dev xvfb xauth xclip libgl1-mesa-dri \
    fonts-noto-cjk python3 && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy
# Use Rust's bundled LLD for low-memory validation instead of GNU BFD.
RUN validation_sysroot=$(rustc --print sysroot) && \
    validation_triple=$(rustc -vV | sed -n 's/^host: //p') && \
    ln -sf "$validation_sysroot/lib/rustlib/$validation_triple/bin/rust-lld" /usr/bin/ld
# winit loads the X11 xkbcommon bridge dynamically; compilation alone misses it.
RUN apt-get update && apt-get install -y --no-install-recommends \
    libxkbcommon-x11-dev fonts-noto-color-emoji && rm -rf /var/lib/apt/lists/*
