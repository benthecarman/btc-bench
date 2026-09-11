import copy
from contextlib import nullcontext
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest

from rl_replay import prepare_replay, replay_trainer_class


class Tokenizer:
    def encode(self, text, **kwargs):
        return [ord(c) - ord('a') for c in text]

    def decode(self, ids):
        return ''.join(chr(i + ord('a')) for i in ids)


ROW = dict(prompt='abc', completion='de', split='training', task_id='train-1', interface='chat')


class ReplayTests(unittest.TestCase):
    def test_rejects_bad_provenance_and_truncation(self):
        for row in [dict(ROW, split='evaluation'), dict(ROW, completion='')]:
            with self.assertRaises(ValueError):
                prepare_replay([row], Tokenizer())
        with self.assertRaises(ValueError):
            prepare_replay([ROW], Tokenizer(), max_length=4)
        row = prepare_replay([ROW], Tokenizer())[0]
        self.assertEqual(row['labels'], [-100, -100, -100, 3, 4])

    def test_replay_gradient_matches_masked_reference_once_per_update(self):
        import torch
        from torch import nn
        from torch.nn import functional as F

        class Model(nn.Module):
            def __init__(self):
                super().__init__()
                self.table = nn.Parameter(torch.arange(25, dtype=torch.float).reshape(5, 5) / 25)

            def forward(self, input_ids, labels, **kwargs):
                logits = self.table[input_ids]
                return SimpleNamespace(loss=F.cross_entropy(logits[:, :-1].reshape(-1, 5),
                                                             labels[:, 1:].reshape(-1)))

        class Base:
            def training_step(self, model, inputs, num_items_in_batch):
                # The base path has already backpropagated its RL loss.
                return torch.tensor(0.)

            def compute_loss_context_manager(self):
                return nullcontext()

        for accumulation in [1, 8]:
            with self.subTest(accumulation=accumulation), TemporaryDirectory() as directory:
                model = Model()
                reference = copy.deepcopy(model)
                trainer = replay_trainer_class(Base)()
                trainer.processing_class = Tokenizer()
                trainer.state = SimpleNamespace(global_step=0)
                trainer.accelerator = SimpleNamespace(num_processes=1, distributed_type='NO',
                    device='cpu', gradient_accumulation_steps=accumulation, sync_gradients=False,
                    backward=lambda loss: (loss / accumulation).backward())
                log = Path(directory) / 'replay.jsonl'
                trainer.configure_replay([ROW, dict(ROW, task_id='train-2')], .1, log,
                                         Path(directory) / 'audit.json')
                for _ in range(7):
                    trainer.training_step(model, {})
                self.assertIsNone(model.table.grad)
                trainer.accelerator.sync_gradients = True
                trainer.training_step(model, {})
                # Only b->c is a prompt prediction; c->d and d->e are supervised.
                expected = .1 * F.cross_entropy(reference.table[torch.tensor([2, 3])], torch.tensor([3, 4]))
                expected.backward()
                torch.testing.assert_close(model.table.grad, reference.table.grad)
                records = [json.loads(line) for line in log.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                self.assertEqual(records[0]['supervised_tokens'], 2)
                trainer.state.global_step = 1
                trainer.training_step(model, {})
                self.assertEqual(json.loads(log.read_text().splitlines()[-1])['task_id'], 'train-2')


if __name__ == '__main__':
    unittest.main()
