FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -y --no-install-recommends python3 libasound2 ca-certificates git ripgrep && rm -rf /var/lib/apt/lists/*
COPY osmogrep /usr/local/bin/osmogrep
COPY provider.py start.sh /fixture/
RUN chmod +x /usr/local/bin/osmogrep /fixture/start.sh
WORKDIR /app
RUN printf 'def mean(values):\n    return sum(values)\n' > maths.py && printf 'USER_NOTE_KEEP_947\n' > notes.txt
ENTRYPOINT ["/fixture/start.sh"]
