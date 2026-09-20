"""Check artifact preservation and server restoration without GPU access."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from sft_experiment import Experiment, comparisons, digest, matches_server, validate_training_rows, verify_hashes


class ExperimentTests(unittest.TestCase):
    def plan(self, root):
        path = root/'plan.json'
        path.write_text(json.dumps(dict(out=str(root/'run'),original=dict(model='original',alias='original'))))
        return path

    def test_existing_output_is_not_rewritten(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            experiment = Experiment(self.plan(root))
            experiment.out.mkdir()
            status = experiment.out/'status.json'
            status.write_text('preserve this run')
            with self.assertRaises(FileExistsError): experiment.run_and_restore()
            self.assertEqual(status.read_text(),'preserve this run')

    def test_training_failure_restores_original_and_keeps_error(self):
        with tempfile.TemporaryDirectory() as temp:
            experiment = Experiment(self.plan(Path(temp)))
            def fail():
                experiment.out.mkdir()
                experiment.owns_output = experiment.switched = True
                raise RuntimeError('training failed')
            class Server:
                pid = 1234
            def serve(*args): experiment.active = Server()
            with patch.object(experiment,'execute',side_effect=fail), \
                 patch.object(experiment,'stop_server'), \
                 patch.object(experiment,'serve',side_effect=serve) as restore:
                with self.assertRaisesRegex(RuntimeError,'training failed'): experiment.run_and_restore()
                restore.assert_called_once_with('original','original','original-restored')
            status = json.loads((experiment.out/'status.json').read_text())
            self.assertEqual(status['phase'],'Failed; original server restored')
            self.assertIn('training failed',status['error'])

    def test_frozen_input_change_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'data.jsonl'
            path.write_text('original')
            hashes = {str(path):digest(path)}
            verify_hashes(hashes)
            path.write_text('different')
            with self.assertRaisesRegex(ValueError,'Frozen input changed'): verify_hashes(hashes)

    def test_missing_answers_count_as_paired_losses(self):
        fixtures = [dict(id='a'),dict(id='b'),dict(id='c')]
        parent = dict(a=dict(score=1),b=dict(score=0,failure='wrong semantics'))
        candidate = dict(b=dict(score=1))
        self.assertEqual(comparisons(fixtures,parent,candidate),
                         dict(both=[],gained=['b'],lost=['a'],neither=['c']))

    def test_server_identity_accepts_storage_symlink(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            real = root/'models'; real.mkdir()
            link = root/'runs'; link.symlink_to(real,target_is_directory=True)
            args = [b'vllm',b'serve',str(link).encode(),b'--served-model-name',b'original',b'--port',b'8010']
            self.assertTrue(matches_server(args,real,'original',8010))
            self.assertFalse(matches_server(args,real,'other',8010))

    def test_writing_plan_rejects_other_tasks_and_interfaces(self):
        plan=dict(rows=1,task_kinds=['write'],training_interfaces=['chat'])
        row=dict(split='training',task_kind='write',interface='chat')
        validate_training_rows([row],plan)
        for kind in ['repair','tree','identify','optimize',None]:
            with self.assertRaisesRegex(ValueError,'excluded task kind'):
                validate_training_rows([dict(row,task_kind=kind)],plan)
        with self.assertRaisesRegex(ValueError,'excluded interface'):
            validate_training_rows([dict(row,interface='submit')],plan)


if __name__=='__main__': unittest.main()
