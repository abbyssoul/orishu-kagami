"""Private, short-lived collector-only certificate fixtures; no service install."""
import ssl


class CollectorTLS:
    def __init__(self, root, run, environment):
        self.directory = root / "collector-tls"
        self.directory.mkdir(mode=0o700)

        def execute(arguments):
            run(["openssl", *arguments], environment, cwd=self.directory)

        # Independent server and exporter-client trust; neither is worker peer trust.
        for name in ("server-ca", "client-ca", "rogue-ca"):
            execute(["req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256",
                     "-nodes", "-days", "1", "-subj", f"/CN=orishu-test-{name}",
                     "-addext", "basicConstraints=critical,CA:TRUE",
                     "-addext", "keyUsage=critical,keyCertSign,cRLSign",
                     "-keyout", f"{name}.key", "-out", f"{name}.crt"])
        for serial, (name, issuer, usage, san) in enumerate((
                ("server", "server-ca", "serverAuth", "IP:127.0.0.1"),
                ("wrong-name", "server-ca", "serverAuth", "DNS:other.invalid"),
                ("client", "client-ca", "clientAuth", "DNS:collector-exporter.invalid"),
                ("rogue", "rogue-ca", "clientAuth", "DNS:rogue-exporter.invalid")), start=1):
            execute(["req", "-new", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256",
                     "-nodes", "-subj", f"/CN=orishu-test-{name}", "-keyout", f"{name}.key",
                     "-out", f"{name}.csr"])
            (self.directory / f"{name}.ext").write_text(
                "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\n"
                f"extendedKeyUsage={usage}\nsubjectAltName={san}\n")
            execute(["x509", "-req", "-in", f"{name}.csr", "-CA", f"{issuer}.crt",
                     "-CAkey", f"{issuer}.key", "-set_serial", str(serial), "-days", "1",
                     "-extfile", f"{name}.ext", "-out", f"{name}.crt"])
        for path in self.directory.iterdir():
            path.chmod(0o600)

    def client_context(self, identity="client"):
        context = ssl.create_default_context(cafile=self.directory / "server-ca.crt")
        context.minimum_version = ssl.TLSVersion.TLSv1_3
        if identity is not None:
            context.load_cert_chain(self.directory / f"{identity}.crt",
                                    self.directory / f"{identity}.key")
        return context

    def worker_arguments(self, failure=None):
        ca = "rogue-ca" if failure == "wrong-server-ca" else "server-ca"
        arguments = ["--tracing.ca-file", str(self.directory / f"{ca}.crt")]
        if failure != "missing-client":
            identity = "rogue" if failure == "wrong-client-ca" else "client"
            arguments += ["--tracing.client-cert-file", str(self.directory / f"{identity}.crt"),
                          "--tracing.client-key-file", str(self.directory / f"{identity}.key")]
        return arguments

    def forbidden_material(self):
        # Also inspect PEM payload encodings, not only recognizable PEM delimiters.
        material = [b"BEGIN CERTIFICATE", b"BEGIN PRIVATE KEY"]
        for path in self.directory.iterdir():
            if path.suffix in (".crt", ".key"):
                raw = path.read_bytes()
                material += [raw, b"".join(line for line in raw.splitlines()
                                           if not line.startswith(b"-----"))]
        return tuple(material)
