from pathlib import Path
import tempfile
import unittest

import coding_grade


class CodingGradeTests(unittest.TestCase):
    def test_documentation_rejects_duplicated_or_removed_user_content(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            good = '# Example\n\nUSER_NOTE_KEEP_947\n\n## Installation\n\npython3 -m pip install example\n'
            for text, expected in [(good, True), ('# Example\n\n' + good, False), (good.replace('USER_NOTE_KEEP_947', ''), False)]:
                (root / 'README.md').write_text(text)
                self.assertEqual(coding_grade.documentation(root), expected)

    def test_mean_rejects_a_hardcoded_visible_test_answer(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'test_maths.py').write_text('import unittest\nfrom maths import mean\nclass Tests(unittest.TestCase):\n def test_mean(self): self.assertEqual(mean([2,4,6]),4)\n')
            (root / 'maths.py').write_text('def mean(values):\n    return 4\n')
            self.assertFalse(coding_grade.bug_fix(root))
            (root / 'maths.py').write_text('def mean(values):\n    return sum(values)/len(values)\n')
            self.assertTrue(coding_grade.bug_fix(root))

    def test_rename_requires_removal_of_old_symbol_and_real_behavior(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'formatting.py').write_text('def format_greeting(name):\n    return "Hello " + name\n')
            (root / 'app.py').write_text('from formatting import format_greeting\ndef render():\n    return format_greeting("world")\n')
            (root / 'test_app.py').write_text('import unittest\nfrom app import render\nclass Tests(unittest.TestCase):\n def test_render(self): self.assertEqual(render(), "Hello world")\n')
            self.assertTrue(coding_grade.multi_file_change(root))
            with (root / 'formatting.py').open('a') as file:
                file.write('def greeting(name):\n    return "wrong"\n')
            self.assertFalse(coding_grade.multi_file_change(root))


if __name__ == '__main__':
    unittest.main()
