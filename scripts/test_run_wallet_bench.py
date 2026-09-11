import unittest
from run_wallet_bench import request_body, submit_answer


class WalletRunnerTests(unittest.TestCase):
    def test_no_answer_keys_or_generation_caps_enter_requests(self):
        fixture = {'request': 'Our human request.', 'reference_template': 'PRIVATE REFERENCE'}
        for mode in ['chat', 'submit']:
            body = request_body(fixture, 'test-model', mode)
            self.assertNotIn('PRIVATE REFERENCE', str(body))
            self.assertNotIn('max_tokens', body)
            self.assertNotIn('max_completion_tokens', body)
            self.assertEqual(body['messages'][1]['content'], fixture['request'])
            self.assertEqual('tools' in body, mode == 'submit')

    def test_multiple_calls_and_plain_text_are_not_rescued(self):
        call = {'function': {'name': 'submit_descriptor', 'arguments': '{"descriptor":"tr(@0/**)"}'}}
        self.assertEqual(submit_answer({'tool_calls': [call]}), 'tr(@0/**)')
        for message in [{'tool_calls': [call, call]}, {'content': 'tr(@0/**)'},
                        {'tool_calls': [{'function': {'name': 'submit_script', 'arguments': '{}'}}]}]:
            with self.assertRaises(ValueError):
                submit_answer(message)


if __name__ == '__main__':
    unittest.main()
