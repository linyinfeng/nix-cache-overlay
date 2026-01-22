# nix-cache-overlay

A naive Nix cache overlay service. Report upstream `narinfo`s to `nix copy`, so that store paths presenting in the upstream will not be copied.

## Usage

Start a cache overlay server:

```nix
{
  services.nix-cache-overlay = {
    enable = true;
    listen = "[::1]:8080";
    endpoint = "https://host.of.s3.endpoint";
    environmentFile = /path/to/env/file;
  };
}
```

Environment file example:

```sh
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...
AWS_EC2_METADATA_DISABLED=true
NIX_CACHE_OVERLAY_TOKEN=...
```

Or from command line:

```bash
# export environment variables, then
nix-cache-overlay --listen "[::1]:8080" --endpoint "https://host.of.s3.endpoint"
```

Sign and push to the cache overlay:

```bash
export AWS_ACCESS_KEY_ID="$NIX_CACHE_OVERLAY_TOKEN"
export AWS_SECRET_ACCESS_KEY="-"
export AWS_EC2_METADATA_DISABLED=true
nix store sign "$STORE_PATH" --recursive --key-file "$KEY_FILE"
nix copy "$STORE_PATH" --to "s3://$BUCKET_NAME?endpoint=$CACHE_OVERLAY_URL"
```

By default upstreams are `["https://cache.nixos.org"]`, you can customize it by setting the `--upstreams` flag (use the `services.nix-cache-overlay.extraArgs` option in NixOS module).
