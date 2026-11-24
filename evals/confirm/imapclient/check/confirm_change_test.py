from imapclient.util import quote_mailbox


def test_quotes():
    assert quote_mailbox("INBOX") == '"INBOX"'
    assert quote_mailbox('Sent "old"') == '"Sent \\"old\\""'
    assert quote_mailbox("a\\b") == '"a\\\\b"'
