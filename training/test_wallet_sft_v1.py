import unittest
from prepare_wallet_sft_v1 import prepare_wallet
from check_wallet_sft_v1_traces import extract


class WalletSftTests(unittest.TestCase):
    def test_evaluation_fixture_cannot_be_exported(self):
        with self.assertRaisesRegex(AssertionError, 'Evaluation fixture'):
            prepare_wallet({'split':'evaluation'}, {}, None, 'chat')

    def test_body_extraction_stops_before_prose_and_keeps_bad_syntax(self):
        self.assertEqual(extract('Taproot leaf Miniscript:\npk(@0/**)\nTaproot internal key supplies a route.')[1], ['pk(@0/**)'])
        self.assertEqual(extract('Taproot leaf Miniscript:\nc:older(4)\nUse a key.')[1], ['c:older(4)'])
        self.assertEqual(extract('Taproot leaf Miniscript:\npk(@0/**)\npk(@1/**)\nThe end.')[1], ['pk(@0/**)', 'pk(@1/**)'])
        self.assertIsNone(extract('Miniscript:\npk(A)\n\nMiniscript:\npk(B)')[1])


if __name__ == '__main__': unittest.main()
