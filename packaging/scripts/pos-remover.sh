#!/bin/sh
set -e
systemctl daemon-reload || true
# "purge" do deb apaga também o histórico.
if [ "$1" = "purge" ]; then
  rm -rf /var/lib/kwanto
fi
