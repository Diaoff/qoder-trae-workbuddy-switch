#!/usr/bin/env bash
# 构建 buddy-switch 服务端镜像并部署到远端 Docker 主机（SSH）。
#
# 用法：
#   scripts/deploy-image.sh --host root@1.2.3.4
#   scripts/deploy-image.sh --host root@1.2.3.4 --keep 4
#   scripts/deploy-image.sh --host root@1.2.3.4 --tag 2026.11.41007-hotfix
#   scripts/deploy-image.sh --host root@1.2.3.4 --no-clean --no-build
#
# 三条不可动摇的约定：
#
# 1. runtime 阶段复用现网镜像作基底。本机网络会破坏 Debian 仓库的签名校验
#    （apt-get update 必然 GPG 失败），所以不能直接 FROM debian 装 ca-certificates/curl。
#    基底 = 服务器 compose 里当前在跑的那个 tag；本地没有就先 docker load 回来。
# 2. tag 带时间戳，绝不覆盖现网 tag。回滚 = 把 compose 的 image 改回上一版。
# 3. 清理永远保留当前 + 上一版。删镜像/部署包是破坏性动作，留够回滚位。
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

HOST=""
REMOTE_DIR="/opt/buddy-switch"
KEEP=3
TAG=""
BASE=""
DO_BUILD=1
DO_CLEAN=1

usage() {
  sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
  if [ $# -gt 0 ]; then exit "$1"; fi
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --host) HOST="$2"; shift 2 ;;
    --dir) REMOTE_DIR="$2"; shift 2 ;;
    --keep) KEEP="$2"; shift 2 ;;
    --tag) TAG="$2"; shift 2 ;;
    --base) BASE="$2"; shift 2 ;;
    --no-build) DO_BUILD=0; shift ;;
    --no-clean) DO_CLEAN=0; shift ;;
    -h|--help) usage 0 ;;
    *) echo "未知参数：$1" >&2; usage 1 ;;
  esac
done

if [ -z "$HOST" ]; then echo "缺少 --host（例如 --host root@1.2.3.4）" >&2; usage 1; fi
command -v docker >/dev/null || { echo "未找到 docker" >&2; exit 1; }
command -v ssh >/dev/null || { echo "未找到 ssh" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "Docker daemon 不可用（colima start？）" >&2; exit 1; }

rssh() { ssh -o BatchMode=yes -o ConnectTimeout=8 "$HOST" "$@"; }
rssh true >/dev/null || { echo "SSH 连不上 $HOST（确认密钥/免密）" >&2; exit 1; }

VERSION=$(node -e 'const fs=require("fs");process.stdout.write(JSON.parse(fs.readFileSync("package.json","utf8")).version)')
if [ -z "$TAG" ]; then TAG="$VERSION-$(date +%m%d%H%M)"; fi
IMAGE="buddy-switch:$TAG"

PREV_IMAGE=$(rssh "grep -m1 -E '^[[:space:]]*image:' '$REMOTE_DIR/docker-compose.yml' | sed 's/.*image:[[:space:]]*//'" | tr -d '\r')
if [ -z "$PREV_IMAGE" ]; then echo "读不到现网 compose 的 image 字段" >&2; exit 1; fi
if [ -z "$BASE" ]; then BASE="$PREV_IMAGE"; fi
echo "==> 版本 $VERSION / 新 tag $TAG"
echo "==> 现网镜像 $PREV_IMAGE"
echo "==> runtime 基底 $BASE"

if ! docker image inspect "$BASE" >/dev/null 2>&1; then
  echo "==> 本地没有 $BASE，从服务器取回（docker save | docker load）"
  rssh "docker save '$BASE' | gzip -1" | docker load
fi

TMP_TAR=$(mktemp -t bs-deploy-XXXXXX.tar.gz)
DOCKERFILE="$ROOT/.deploy.Dockerfile"
cleanup_local() { rm -f "$TMP_TAR" "$DOCKERFILE"; }
trap cleanup_local EXIT
if [ "$DO_BUILD" = 1 ]; then
  cat > "$DOCKERFILE" <<'DOCKERFILE_EOF'
# 由 scripts/deploy-image.sh 生成（在构建上下文内，脚本退出时删除）。
# runtime 直接复用现网镜像：本机网络无法通过 Debian 仓库的签名校验。
FROM node:22-bookworm-slim AS frontend
WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM rust:1.98-bookworm AS builder
WORKDIR /src
RUN mkdir -p /usr/local/cargo \
    && printf '%s\n' \
        '[source.crates-io]' \
        'replace-with = "rsproxy-sparse"' \
        '[source.rsproxy-sparse]' \
        'registry = "sparse+https://rsproxy.cn/index/"' \
        '[registries.rsproxy]' \
        'index = "https://rsproxy.cn/crates.io-index"' \
        > /usr/local/cargo/config.toml
COPY Cargo.toml Cargo.lock ./
COPY crates/buddy-switch-core/Cargo.toml ./crates/buddy-switch-core/
COPY crates/buddy-switch-gateway/Cargo.toml ./crates/buddy-switch-gateway/
COPY crates/buddy-switch-server/Cargo.toml ./crates/buddy-switch-server/
COPY crates/buddy-switch-vault/Cargo.toml ./crates/buddy-switch-vault/
COPY src-tauri/Cargo.toml ./src-tauri/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    for crate in buddy-switch-core buddy-switch-gateway buddy-switch-vault; do \
        mkdir -p crates/$crate/src && touch crates/$crate/src/lib.rs; \
    done \
    && mkdir -p crates/buddy-switch-server/src && touch crates/buddy-switch-server/src/main.rs \
    && mkdir -p src-tauri/src && touch src-tauri/src/main.rs src-tauri/src/lib.rs \
    && cargo fetch --locked

COPY . .
COPY --from=frontend /src/dist ./dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p buddy-switch-server \
    && strip target/release/buddy-switch \
    && cp target/release/buddy-switch /tmp/buddy-switch

FROM __RUNTIME_BASE__ AS runtime
COPY --chmod=0755 --from=builder /tmp/buddy-switch /usr/local/bin/buddy-switch
DOCKERFILE_EOF
  perl -pi -e "s|__RUNTIME_BASE__|$BASE|g" "$DOCKERFILE"
  echo "==> 构建 $IMAGE"
  docker build -f "$DOCKERFILE" -t "$IMAGE" .
else
  docker image inspect "$IMAGE" >/dev/null 2>&1 || { echo "本地没有 $IMAGE，去掉 --no-build" >&2; exit 1; }
fi

echo "==> 冒烟：$IMAGE version"
docker run --rm "$IMAGE" version

echo "==> 导出镜像"
docker save "$IMAGE" | gzip -1 > "$TMP_TAR"
ls -lh "$TMP_TAR"

echo "==> 上传并 load"
scp -o BatchMode=yes "$TMP_TAR" "$HOST:$REMOTE_DIR/bs-deploy-$TAG.tar.gz"
rssh "gzip -dc '$REMOTE_DIR/bs-deploy-$TAG.tar.gz' | docker load"

echo "==> 切换 compose 并重启"
rssh "set -e; cd '$REMOTE_DIR'; ts=\$(date +%Y%m%d-%H%M%S); cp -a docker-compose.yml docker-compose.yml.bak-\$ts; sed -i 's|image: buddy-switch:.*|image: $IMAGE|' docker-compose.yml; docker compose up -d 2>&1 | tail -5; echo backup=docker-compose.yml.bak-\$ts"

echo "==> 等待 healthy"
status=""
for _ in $(seq 1 30); do
  status=$(rssh "docker inspect -f '{{.State.Health.Status}}' buddy-switch" 2>/dev/null || echo unknown)
  if [ "$status" = "healthy" ]; then break; fi
  sleep 2
done
if [ "$status" != "healthy" ]; then
  echo "!! 容器未 healthy（当前 $status），最近日志：" >&2
  rssh "docker logs --tail 40 buddy-switch" >&2 || true
  echo "回滚：把 $REMOTE_DIR/docker-compose.yml 的 image 改回 $PREV_IMAGE 后 docker compose up -d" >&2
  exit 1
fi

echo "==> 校验"
rssh "docker ps --filter name=buddy-switch --format '{{.Image}} | {{.Status}} | {{.Ports}}'; docker exec buddy-switch buddy-switch version; docker exec buddy-switch curl -fsS -o /dev/null -w 'healthz=%{http_code}\n' http://127.0.0.1:57890/healthz"

if [ "$DO_CLEAN" = 1 ]; then
  echo "==> 清理（镜像 / 部署包各留 $KEEP 个，且当前与上一版永不删除）"
  rssh "bash -s -- '$TAG' '$PREV_IMAGE' '$KEEP' '$REMOTE_DIR'" <<'REMOTE_CLEAN_EOF'
set -euo pipefail
TAG=$1
PREV=$2
KEEP=$3
DIR=$4
cd "$DIR"

ls -1t bs-deploy-*.tar.gz 2>/dev/null | tail -n +$((KEEP + 1)) | while read -r f; do
  rm -f "$f"
done

docker images buddy-switch --format '{{.Tag}} {{.CreatedAt}}' | sort -k2,3 -r | awk '{print $1}' \
  | tail -n +$((KEEP + 1)) | while read -r t; do
      if [ "$t" = "<none>" ]; then continue; fi
      if [ "buddy-switch:$t" = "$TAG" ]; then continue; fi
      if [ "buddy-switch:$t" = "$PREV" ]; then continue; fi
      docker rmi "buddy-switch:$t" >/dev/null 2>&1 || true
    done

docker image prune -f >/dev/null
REMOTE_CLEAN_EOF
fi

echo
echo "完成：$IMAGE"
echo "回滚：把 compose 的 image 改回 $PREV_IMAGE 后 docker compose up -d"
