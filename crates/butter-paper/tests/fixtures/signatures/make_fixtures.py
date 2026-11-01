"""Signed PDF fixtures for the digital signature tests, made with pyHanko.

Run with pyHanko 0.29 (`pip install "pyhanko==0.29.*"`) from the repository
root:

    python crates/butter-paper/tests/fixtures/signatures/make_fixtures.py

Every certificate and key here is made up for the tests: a test root, an
RSA signer, an ECDSA (P-256) signer and a timestamp authority. The signed
document is `text-edit/word-table.pdf`, whose text is made up too.
"""

import datetime
import pathlib

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, rsa
from cryptography.hazmat.primitives.serialization import pkcs12
from cryptography.x509 import ocsp
from cryptography.x509.oid import AuthorityInformationAccessOID, ExtendedKeyUsageOID, NameOID

from asn1crypto import keys as asn1_keys
from asn1crypto import x509 as asn1_x509
from pyhanko.pdf_utils import generic
from pyhanko.pdf_utils.incremental_writer import IncrementalPdfFileWriter
from pyhanko.sign import fields, signers, timestamps
from pyhanko.sign.signers.pdf_signer import PdfSignatureMetadata
from pyhanko_certvalidator.registry import SimpleCertificateStore

HERE = pathlib.Path(__file__).resolve().parent
SOURCE = HERE.parent / "text-edit" / "word-table.pdf"
NOT_BEFORE = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
NOT_AFTER = datetime.datetime(2046, 1, 1, tzinfo=datetime.timezone.utc)
P12_PASSWORD = b"butter-paper-test"
# Where the signers' certificates say their revocation status is; the tests
# answer these addresses themselves.
OCSP_URL = "http://ocsp.butter-paper.test"
CRL_URL = "http://crl.butter-paper.test/root.crl"
REVOKED_AT = datetime.datetime(2026, 2, 1, tzinfo=datetime.timezone.utc)


def name(common_name):
    return x509.Name(
        [
            x509.NameAttribute(NameOID.COMMON_NAME, common_name),
            x509.NameAttribute(NameOID.ORGANIZATION_NAME, "Butter Paper Tests"),
            x509.NameAttribute(NameOID.COUNTRY_NAME, "AU"),
        ]
    )


def certificate(subject, key, issuer, issuer_key, ca=False, usage=None, revocation=False):
    builder = (
        x509.CertificateBuilder()
        .subject_name(name(subject))
        .issuer_name(name(issuer))
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(NOT_BEFORE)
        .not_valid_after(NOT_AFTER)
        .add_extension(x509.BasicConstraints(ca=ca, path_length=None), critical=True)
        .add_extension(
            x509.SubjectKeyIdentifier.from_public_key(key.public_key()), critical=False
        )
    )
    if ca:
        builder = builder.add_extension(
            x509.KeyUsage(False, False, False, False, False, True, True, False, False),
            critical=True,
        )
    else:
        builder = builder.add_extension(
            x509.KeyUsage(True, True, False, False, False, False, False, False, False),
            critical=True,
        )
    if usage:
        builder = builder.add_extension(x509.ExtendedKeyUsage(usage), critical=True)
    if revocation:
        builder = builder.add_extension(
            x509.AuthorityInformationAccess(
                [
                    x509.AccessDescription(
                        AuthorityInformationAccessOID.OCSP,
                        x509.UniformResourceIdentifier(OCSP_URL),
                    )
                ]
            ),
            critical=False,
        ).add_extension(
            x509.CRLDistributionPoints(
                [
                    x509.DistributionPoint(
                        [x509.UniformResourceIdentifier(CRL_URL)], None, None, None
                    )
                ]
            ),
            critical=False,
        )
    return builder.sign(issuer_key, hashes.SHA256())


def asn1_certificate(cert):
    return asn1_x509.Certificate.load(cert.public_bytes(serialization.Encoding.DER))


def asn1_key(key):
    return asn1_keys.PrivateKeyInfo.load(
        key.private_bytes(
            serialization.Encoding.DER,
            serialization.PrivateFormat.PKCS8,
            serialization.NoEncryption(),
        )
    )


def signer(cert, key, root):
    return signers.SimpleSigner(
        signing_cert=asn1_certificate(cert),
        signing_key=asn1_key(key),
        cert_registry=SimpleCertificateStore.from_certs([asn1_certificate(root)]),
    )


def sign(source, out, simple_signer, field, timestamper=None, box=None, reason=None):
    with open(source, "rb") as file:
        writer = IncrementalPdfFileWriter(file, strict=False)
        if box:
            fields.append_signature_field(
                writer, fields.SigFieldSpec(field, box=box, on_page=0)
            )
        metadata = PdfSignatureMetadata(
            field_name=field,
            reason=reason,
            location="Melbourne",
            md_algorithm="sha256",
        )
        with open(out, "wb") as output:
            signers.sign_pdf(
                writer,
                metadata,
                signer=simple_signer,
                timestamper=timestamper,
                output=output,
            )


def add_note(source, out):
    """A later revision that only adds a note: a change signatures permit."""
    with open(source, "rb") as file:
        writer = IncrementalPdfFileWriter(file, strict=False)
        page = writer.root["/Pages"]["/Kids"][0]
        page_ref = writer.root["/Pages"]["/Kids"].raw_get(0)
        note = writer.add_object(
            generic.DictionaryObject(
                {
                    generic.NameObject("/Type"): generic.NameObject("/Annot"),
                    generic.NameObject("/Subtype"): generic.NameObject("/Text"),
                    generic.NameObject("/Rect"): generic.ArrayObject(
                        [generic.FloatObject(v) for v in (40, 40, 60, 60)]
                    ),
                    generic.NameObject("/Contents"): generic.TextStringObject(
                        "Reviewed"
                    ),
                }
            )
        )
        annots = page.get("/Annots")
        annots = generic.ArrayObject(list(annots) if annots else [])
        annots.append(note)
        page[generic.NameObject("/Annots")] = annots
        writer.update_container(page)
        writer.mark_update(page_ref)
        with open(out, "wb") as output:
            writer.write(output)


def change_content(source, out):
    """A later revision that draws over the page: a change to its content."""
    with open(source, "rb") as file:
        writer = IncrementalPdfFileWriter(file, strict=False)
        page = writer.root["/Pages"]["/Kids"][0]
        page_ref = writer.root["/Pages"]["/Kids"].raw_get(0)
        stream = generic.StreamObject(stream_data=b"1 0 0 rg 72 72 200 40 re f")
        contents = page["/Contents"]
        if isinstance(contents, generic.ArrayObject):
            contents = generic.ArrayObject(
                [contents.raw_get(i) for i in range(len(contents))]
            )
        else:
            contents = generic.ArrayObject([page.raw_get("/Contents")])
        contents.append(writer.add_object(stream))
        page[generic.NameObject("/Contents")] = contents
        writer.mark_update(page_ref)
        with open(out, "wb") as output:
            writer.write(output)


def tamper(source, out):
    """A byte inside the signed range changed after signing."""
    data = bytearray(source.read_bytes())
    marker = data.index(b"/MediaBox")
    data[marker + 1] = ord("m")
    out.write_bytes(bytes(data))


def main():
    root_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    root = certificate(
        "Butter Paper Test Root", root_key, "Butter Paper Test Root", root_key, ca=True
    )
    rsa_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    rsa_cert = certificate(
        "Test Signer RSA", rsa_key, "Butter Paper Test Root", root_key, revocation=True
    )
    revoked_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    revoked_cert = certificate(
        "Test Signer Revoked", revoked_key, "Butter Paper Test Root", root_key, revocation=True
    )
    ec_key = ec.generate_private_key(ec.SECP256R1())
    ec_cert = certificate("Test Signer EC", ec_key, "Butter Paper Test Root", root_key)
    tsa_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    tsa_cert = certificate(
        "Test Timestamp Authority",
        tsa_key,
        "Butter Paper Test Root",
        root_key,
        usage=[ExtendedKeyUsageOID.TIME_STAMPING],
    )

    (HERE / "test-root.cer").write_bytes(root.public_bytes(serialization.Encoding.DER))
    (HERE / "test-signer-rsa.p12").write_bytes(
        pkcs12.serialize_key_and_certificates(
            b"Test Signer RSA",
            rsa_key,
            rsa_cert,
            [root],
            serialization.BestAvailableEncryption(P12_PASSWORD),
        )
    )
    (HERE / "test-signer-ec.p12").write_bytes(
        pkcs12.serialize_key_and_certificates(
            b"Test Signer EC",
            ec_key,
            ec_cert,
            [root],
            serialization.BestAvailableEncryption(P12_PASSWORD),
        )
    )

    # Legacy encryption (3DES, SHA-1 MAC), as older Windows and macOS
    # exports write and macOS's keychain import still needs.
    legacy = (
        serialization.PrivateFormat.PKCS12.encryption_builder()
        .kdf_rounds(2048)
        .key_cert_algorithm(pkcs12.PBES.PBESv1SHA1And3KeyTripleDESCBC)
        .hmac_hash(hashes.SHA1())
        .build(P12_PASSWORD)
    )
    for name, key, cert in [
        ("test-signer-rsa-legacy.p12", rsa_key, rsa_cert),
        ("test-signer-ec-legacy.p12", ec_key, ec_cert),
    ]:
        (HERE / name).write_bytes(
            pkcs12.serialize_key_and_certificates(name.encode(), key, cert, [root], legacy)
        )
    (HERE / "test-signer-revoked.p12").write_bytes(
        pkcs12.serialize_key_and_certificates(
            b"Test Signer Revoked",
            revoked_key,
            revoked_cert,
            [root],
            serialization.BestAvailableEncryption(P12_PASSWORD),
        )
    )
    # The root's revocation list, and its OCSP answers for both signers.
    now = datetime.datetime.now(datetime.timezone.utc)
    crl = (
        x509.CertificateRevocationListBuilder()
        .issuer_name(root.subject)
        .last_update(now - datetime.timedelta(days=1))
        .next_update(NOT_AFTER)
        .add_revoked_certificate(
            x509.RevokedCertificateBuilder()
            .serial_number(revoked_cert.serial_number)
            .revocation_date(REVOKED_AT)
            .build()
        )
        .sign(root_key, hashes.SHA256())
    )
    (HERE / "test-root.crl").write_bytes(crl.public_bytes(serialization.Encoding.DER))
    for name, cert, status in [
        ("ocsp-rsa-good.der", rsa_cert, ocsp.OCSPCertStatus.GOOD),
        ("ocsp-revoked.der", revoked_cert, ocsp.OCSPCertStatus.REVOKED),
    ]:
        response = (
            ocsp.OCSPResponseBuilder()
            .add_response(
                cert=cert,
                issuer=root,
                algorithm=hashes.SHA1(),
                cert_status=status,
                this_update=now - datetime.timedelta(hours=1),
                next_update=NOT_AFTER,
                revocation_time=REVOKED_AT if status == ocsp.OCSPCertStatus.REVOKED else None,
                revocation_reason=None,
            )
            .responder_id(ocsp.OCSPResponderEncoding.HASH, root)
            .sign(root_key, hashes.SHA256())
        )
        (HERE / name).write_bytes(response.public_bytes(serialization.Encoding.DER))

    rsa_signer = signer(rsa_cert, rsa_key, root)
    ec_signer = signer(ec_cert, ec_key, root)
    tsa = timestamps.DummyTimeStamper(
        tsa_cert=asn1_certificate(tsa_cert),
        tsa_key=asn1_key(tsa_key),
        certs_to_embed=SimpleCertificateStore.from_certs([asn1_certificate(root)]),
    )

    signed = HERE / "signed-rsa.pdf"
    sign(SOURCE, signed, rsa_signer, "Signature1", reason="Approved")
    sign(
        SOURCE,
        HERE / "signed-ec-visible.pdf",
        ec_signer,
        "Signature1",
        box=(360, 60, 560, 120),
    )
    sign(SOURCE, HERE / "signed-timestamped.pdf", rsa_signer, "Signature1", timestamper=tsa)
    sign(signed, HERE / "signed-twice.pdf", ec_signer, "Signature2", box=(360, 60, 560, 120))
    add_note(signed, HERE / "signed-then-noted.pdf")
    change_content(signed, HERE / "signed-then-changed.pdf")
    tamper(signed, HERE / "signed-tampered.pdf")
    sign(SOURCE, HERE / "signed-revoked.pdf", signer(revoked_cert, revoked_key, root), "Signature1")


if __name__ == "__main__":
    main()
