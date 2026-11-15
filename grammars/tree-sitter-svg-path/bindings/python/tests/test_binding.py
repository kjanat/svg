from unittest import TestCase

import tree_sitter_svg_path

from tree_sitter import Language, Parser


class TestLanguage(TestCase):
    def test_can_parse_input(self) -> None:
        parser = Parser(Language(tree_sitter_svg_path.language()))
        source = b"M0 0 L10 10 Z"
        # tree-sitter 0.25 exposes a partially unknown parse overload on Python 3.15.
        tree = parser.parse(source)  # pyright: ignore[reportUnknownMemberType]
        self.assertFalse(tree.root_node.has_error)
        self.assertEqual(tree.root_node.end_byte, len(source))
        self.assertGreater(tree.root_node.named_child_count, 0)
