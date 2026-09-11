"""Add one completion-only SFT example to each GRPO optimizer update.

The two backwards run separately to release the RL graph before SFT. This
helper supports one process with the normal Accelerate backend only.
"""
import json
from pathlib import Path


def prepare_replay(rows, tokenizer, max_length=4096):
    prepared = []
    for index, row in enumerate(rows):
        if row.get("split") != "training":
            raise ValueError("Replay requires explicit training provenance")
        prompt = tokenizer.encode(row["prompt"], add_special_tokens=False)
        ids = tokenizer.encode(row["prompt"] + row["completion"], add_special_tokens=False)
        if not prompt or ids[:len(prompt)] != prompt:
            raise ValueError("Replay prompt token boundary changed")
        if not len(prompt) < len(ids) <= max_length:
            raise ValueError("Replay completion is empty or exceeds the length limit")
        if tokenizer.decode(ids[len(prompt):]) != row["completion"]:
            raise ValueError("Replay completion changed during tokenization")
        prepared.append(dict(input_ids=ids, labels=[-100] * len(prompt) + ids[len(prompt):],
                             index=index, task_id=row["task_id"], interface=row["interface"],
                             prompt_tokens=len(prompt), supervised_tokens=len(ids)-len(prompt)))
    if not prepared:
        raise ValueError("Replay dataset is empty")
    return prepared


def replay_trainer_class(base):
    class ReplayGRPOTrainer(base):
        def configure_replay(self, rows, weight, log_path, audit_path):
            if weight <= 0:
                raise ValueError("Replay weight must be positive")
            if self.accelerator.num_processes != 1 or str(self.accelerator.distributed_type).split('.')[-1] != "NO":
                raise ValueError("Replay supports one process without distributed backends")
            self.replay_rows = prepare_replay(rows, self.processing_class)
            self.replay_weight = weight
            self.replay_log = Path(log_path)
            self.replay_log.open("x").close()
            with Path(audit_path).open("x") as stream:
                json.dump(self.replay_rows, stream)
                stream.write("\n")

        def training_step(self, model, inputs, num_items_in_batch=None):
            result = super().training_step(model, inputs, num_items_in_batch)
            # Trainer sets this on the last microbatch, before clipping/step.
            if not self.accelerator.sync_gradients:
                return result
            import torch
            import time
            started = time.perf_counter()
            row = self.replay_rows[self.state.global_step % len(self.replay_rows)]
            device = self.accelerator.device
            batch = {key: torch.tensor([row[key]], device=device) for key in ["input_ids", "labels"]}
            batch["attention_mask"] = torch.ones_like(batch["input_ids"])
            with self.compute_loss_context_manager():
                loss = model(**batch, use_cache=False).loss
            if not torch.isfinite(loss):
                raise ValueError("Non-finite replay loss")
            # Accelerate divides by its own accumulation factor. There is one
            # SFT example per full update, so cancel that division explicitly.
            self.accelerator.backward(loss * self.replay_weight * self.accelerator.gradient_accumulation_steps)
            value = loss.detach().float().item()
            with self.replay_log.open("a") as stream:
                stream.write(json.dumps(dict(step=self.state.global_step + 1,
                    index=row["index"], task_id=row["task_id"], interface=row["interface"],
                    prompt_tokens=row["prompt_tokens"], supervised_tokens=row["supervised_tokens"],
                    loss=value, weight=self.replay_weight, seconds=time.perf_counter()-started)) + "\n")
            return result + self.replay_weight * loss.detach()

    return ReplayGRPOTrainer
