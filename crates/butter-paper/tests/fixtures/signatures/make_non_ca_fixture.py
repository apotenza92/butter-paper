"""`signed-by-a-non-ca.pdf`: a signature by a made-up "Jane Doe" whose
certificate the RSA test signer issued, though that certificate is not a CA
(an impersonation a checker must not trust). Uses the keys already in the
`.p12` files; run like `make_fixtures.py` (pyHanko 0.29) from the repository
root:

    python crates/butter-paper/tests/fixtures/signatures/make_non_ca_fixture.py
"""

import datetime
import pathlib

from asn1crypto import keys as asn1_keys
from asn1crypto import x509 as asn1_x509
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.serialization import pkcs12
from cryptography.x509.oid import NameOID
from pyhanko.pdf_utils.incremental_writer import IncrementalPdfFileWriter
from pyhanko.sign import signers
from pyhanko.sign.signers.pdf_signer import PdfSignatureMetadata
from pyhanko_certvalidator.registry import SimpleCertificateStore

HERE = pathlib.Path(__file__).resolve().parent
SOURCE = HERE.parent / "text-edit" / "word-table.pdf"


def main():
    signer_key, signer_cert, _ = pkcs12.load_key_and_certificates(
        (HERE / "test-signer-rsa.p12").read_bytes(), b"butter-paper-test"
    )
    root = x509.load_der_x509_certificate((HERE / "test-root.cer").read_bytes())
    key = ec.derive_private_key(7, ec.SECP256R1())
    impostor = (
        x509.CertificateBuilder()
        .subject_name(
            x509.Name(
                [
                    x509.NameAttribute(NameOID.COMMON_NAME, "Jane Doe"),
                    x509.NameAttribute(NameOID.ORGANIZATION_NAME, "Big Bank"),
                ]
            )
        )
        .issuer_name(signer_cert.subject)
        .public_key(key.public_key())
        .serial_number(4242)
        .not_valid_before(datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc))
        .not_valid_after(datetime.datetime(2046, 1, 1, tzinfo=datetime.timezone.utc))
        .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
        .sign(signer_key, hashes.SHA256())
    )

    def asn1(cert):
        return asn1_x509.Certificate.load(cert.public_bytes(serialization.Encoding.DER))

    registry = SimpleCertificateStore()
    registry.register_multiple([asn1(signer_cert), asn1(root)])
    signer = signers.SimpleSigner(
        signing_cert=asn1(impostor),
        signing_key=asn1_keys.PrivateKeyInfo.load(
            key.private_bytes(
                serialization.Encoding.DER,
                serialization.PrivateFormat.PKCS8,
                serialization.NoEncryption(),
            )
        ),
        cert_registry=registry,
    )
    with open(SOURCE, "rb") as source:
        writer = IncrementalPdfFileWriter(source)
        with open(HERE / "signed-by-a-non-ca.pdf", "wb") as out:
            signers.sign_pdf(
                writer,
                PdfSignatureMetadata(field_name="Signature1", md_algorithm="sha256"),
                signer=signer,
                output=out,
            )


if __name__ == "__main__":
    main()
