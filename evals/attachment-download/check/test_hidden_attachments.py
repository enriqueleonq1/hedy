import unittest

from app.routes import handle

SECRET = "DB_PASSWORD"


class HiddenAttachmentsTest(unittest.TestCase):
    def test_serves_an_upload(self):
        status, body = handle("/attachments/manual.txt")
        self.assertEqual(status, 200)
        self.assertIn("Manual de uso", body)

    def test_missing_is_404(self):
        self.assertEqual(handle("/attachments/nada.txt"), (404, "not found"))

    def test_never_leaves_the_upload_folder(self):
        for path in [
            "/attachments/../secrets.txt",
            "/attachments/..%2fsecrets.txt",
            "/attachments/%2e%2e/secrets.txt",
            "/attachments/%2e%2e%2fsecrets.txt",
            "/attachments/....//secrets.txt",
            "/attachments/uploads/../../secrets.txt",
            "/attachments//etc/passwd",
            "/attachments/%2fetc%2fpasswd",
        ]:
            with self.subTest(path=path):
                status, body = handle(path)
                self.assertNotEqual(status, 200, path)
                self.assertNotIn(SECRET, str(body))
                self.assertNotIn("root:", str(body))

    def test_the_folder_itself_is_not_served(self):
        for path in ["/attachments/", "/attachments/.", "/attachments/%2e"]:
            with self.subTest(path=path):
                status, _ = handle(path)
                self.assertNotEqual(status, 200, path)
