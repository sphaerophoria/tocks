FROM rabits/qt:5.15-desktop

USER root
RUN apt update && apt install -y --no-install-recommends \
	libvpx-dev \
	libopus-dev \
	libclang-dev \
	libsodium-dev \
	libconfig-dev \
	clang \
	libsqlite3-dev \
	libasound2-dev \
	libxcb-icccm4 \
	libxcb-image0 \
	libxcb-keysyms1 \
	libxcb-render-util0 \ 
	libxcb-xinerama0


USER user
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
