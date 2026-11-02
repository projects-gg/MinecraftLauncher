// One thing at a time: the peer connection's negotiations (an offer is answered only after the
// previous answer was applied, a new connection waits for the old one's last step) and the audio's
// start and stop run in the order the launcher sent them. A failed step does not stop the next.

export class SerialQueue {
  private tail: Promise<unknown> = Promise.resolve();

  run<T>(task: () => Promise<T>): Promise<T> {
    const next = this.tail.then(task, task);
    this.tail = next.catch(() => undefined);
    return next;
  }

  /** Resolves once everything queued so far has run. */
  idle(): Promise<void> {
    return this.tail.then(() => undefined);
  }
}
