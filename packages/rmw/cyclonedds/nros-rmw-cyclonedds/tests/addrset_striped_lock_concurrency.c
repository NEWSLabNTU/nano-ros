/* Concurrency test for CycloneDDS' striped addrset locks (nano-ros issue 0496).
 *
 * The addrset lock used to live in the addrset. It now comes from a fixed array
 * of stripes keyed on the addrset address, which means two unrelated addrsets
 * can share one non-recursive mutex. That is safe only if nothing holds two
 * addrset locks at once, and nothing holds one across a callback: a same-thread
 * re-acquire of a shared mutex is a deadlock, not a wait. Two paths in
 * q_addrset.c were restructured to satisfy that, and this test exercises both.
 *
 *   1. copy_addrset_into_addrset_{uc,mc} — locks source AND destination for the
 *      whole walk. It must acquire in a canonical order, or two threads copying
 *      A->B and B->A at the same time form a cycle; and it must collapse to a
 *      single acquire when both addrsets land in the same stripe, or it
 *      self-deadlocks.
 *
 *   2. addrset_forall / addrset_forall_count — must NOT hold the lock while
 *      running the caller's callback. Callbacks re-enter this layer on the same
 *      thread in real code (purge_helper deletes a proxy participant, and
 *      writing that participant's builtin-topic sample calls addrset_forall
 *      again), so a callback touching a same-stripe addrset would deadlock.
 *
 * Neither hazard needs an unlucky schedule — a stripe collision is enough — so
 * this does not rely on winning a race. It builds far more addrsets than there
 * are stripes, which makes collisions certain by pigeonhole, then drives every
 * ordered pair. What it DOES need is a deadlock detector, because the failure
 * mode is a hang rather than a wrong answer: a watchdog fails the test instead
 * of letting CTest sit on it until the suite times out.
 *
 * Written in C, not C++ like its siblings, because the internal headers it needs
 * are C-only: ddsi_domaingv.h carries `typedef struct os_sockWaitset
 * *os_sockWaitset`, which is legal C and a conflicting declaration in C++.
 *
 * No DDS domain is required. add_xlocator_to_addrset only consults gv through
 * ddsi_is_mcaddr, which walks gv's transport-factory list and answers "not
 * multicast" when nothing matches, so a zeroed ddsi_domaingv routes every
 * locator to the unicast tree and never dereferences anything else.
 */

#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include "dds/ddsi/ddsi_domaingv.h"
#include "dds/ddsi/ddsi_locator.h"
#include "dds/ddsi/q_addrset.h"

/* Comfortably more than the 64 stripes in q_addrset.c, so some pair of these
   addrsets is guaranteed to share a mutex however the hash is spelled. The test
   never needs to know WHICH pair, which keeps it from duplicating the hash. */
#define N_ADDRSETS 128
#define N_THREADS 8
#define N_ROUNDS 40
#define LOCATORS_PER_ADDRSET 3

/* A hang is the failure we are hunting, so every phase runs under a deadline. */
#define PHASE_TIMEOUT_SECS 60

static struct ddsi_domaingv *g_gv;
static struct addrset *g_as[N_ADDRSETS];

static void fail (const char *what)
{
  fprintf (stderr, "FAIL: %s\n", what);
  fflush (stderr);
  exit (1);
}

/* A hung worker cannot be joined, so report and leave immediately.
 *
 * `moved` says whether the workers' progress counter advanced while the deadline
 * was expiring. Without it, "0/8 workers done" cannot tell a deadlock from a
 * phase that is merely too slow — and that distinction is the whole point of the
 * test, so the watchdog has to make it rather than leave it to the reader. */
static void fail_hung (const char *phase, int done, int moved)
{
  fflush (stdout); /* _exit skips this, and the passing phases are useful context */
  if (moved)
    fprintf (stderr,
             "FAIL: %s exceeded %d s — %d/%d workers done, but the progress counter WAS\n"
             "still advancing. That is not a deadlock: the phase is too slow, which is a\n"
             "problem with this test's sizing rather than with the locking.\n",
             phase, PHASE_TIMEOUT_SECS, done, N_THREADS);
  else
    fprintf (stderr,
             "FAIL: %s exceeded %d s — %d/%d workers done and the progress counter did\n"
             "NOT advance: the workers are STUCK.\n"
             "This is the signature of a stripe-collision deadlock: two addrsets sharing\n"
             "one mutex, re-acquired on a single thread. Check the LOCK2 ordering in\n"
             "copy_addrset_into_addrset_*, and that addrset_forall_* still snapshots\n"
             "instead of running the callback under the lock (nano-ros issue 0496).\n",
             phase, PHASE_TIMEOUT_SECS, done, N_THREADS);
  fflush (stderr);
  _exit (1);
}

static ddsi_xlocator_t make_locator (int seed)
{
  ddsi_xlocator_t x;
  memset (&x, 0, sizeof (x));
  /* Any kind no factory claims: ddsi_is_mcaddr answers 0 and the locator lands
     in the unicast tree. Must not be NN_LOCATOR_KIND_INVALID, or
     add_xlocator_to_addrset drops it as unspecified. */
  x.c.kind = 1; /* NN_LOCATOR_KIND_UDPv4 */
  x.c.port = (uint32_t) (7400 + seed);
  for (int i = 0; i < 4; i++)
    x.c.address[12 + i] = (unsigned char) ((seed >> (8 * i)) & 0xff);
  /* add_xlocator_to_addrset asserts conn != NULL but never dereferences it on
     this path; compare_xlocators only compares the pointer value. */
  x.conn = (struct ddsi_tran_conn *) (uintptr_t) 0x1000;
  return x;
}

/* ---- phase runner with a deadlock watchdog -------------------------------- */

typedef void (*phase_body_t) (int thread_index);

static pthread_mutex_t g_done_lock = PTHREAD_MUTEX_INITIALIZER;
static int g_done;
static phase_body_t g_body;
static size_t g_callback_hits;       /* guarded by g_done_lock */
static unsigned long long g_progress; /* guarded by g_done_lock */

static void *worker (void *varg)
{
  const int t = (int) (intptr_t) varg;
  g_body (t);
  pthread_mutex_lock (&g_done_lock);
  g_done++;
  pthread_mutex_unlock (&g_done_lock);
  return NULL;
}

static void run_phase (const char *phase, phase_body_t body)
{
  pthread_t th[N_THREADS];
  g_body = body;
  pthread_mutex_lock (&g_done_lock);
  g_done = 0;
  g_progress = 0;
  g_callback_hits = 0;
  pthread_mutex_unlock (&g_done_lock);

  for (int t = 0; t < N_THREADS; t++)
  {
    if (pthread_create (&th[t], NULL, worker, (void *) (intptr_t) t) != 0)
      fail ("pthread_create failed");
  }

  const time_t deadline = time (NULL) + PHASE_TIMEOUT_SECS;
  unsigned long long last_progress = 0;
  for (;;)
  {
    pthread_mutex_lock (&g_done_lock);
    const int done = g_done;
    const unsigned long long progress = g_progress;
    pthread_mutex_unlock (&g_done_lock);
    if (done == N_THREADS)
      break;
    if (time (NULL) > deadline)
      fail_hung (phase, done, progress != last_progress);
    last_progress = progress;
    struct timespec ts = { .tv_sec = 0, .tv_nsec = 20 * 1000 * 1000 };
    nanosleep (&ts, NULL);
  }
  for (int t = 0; t < N_THREADS; t++)
    pthread_join (th[t], NULL);
  printf ("  %s: ok\n", phase);
  fflush (stdout);
}

/* Cheap enough to call once per outer iteration, not per callback. */
static void note_progress (void)
{
  pthread_mutex_lock (&g_done_lock);
  g_progress++;
  pthread_mutex_unlock (&g_done_lock);
}

static void note_callback_hit (void)
{
  pthread_mutex_lock (&g_done_lock);
  g_callback_hits++;
  pthread_mutex_unlock (&g_done_lock);
}

static size_t callback_hits (void)
{
  pthread_mutex_lock (&g_done_lock);
  const size_t n = g_callback_hits;
  pthread_mutex_unlock (&g_done_lock);
  return n;
}

/* ---- hazard 1: two locks held at once, in both directions ----------------- */

/* Thread t copies i -> i+t+1 while other threads copy at other offsets, so
   opposite-direction pairs overlap in time. Every distinct offset is covered,
   so the same-stripe pairs are too. */
static void body_bidirectional_copy (int t)
{
  for (int round = 0; round < N_ROUNDS; round++)
  {
    for (int i = 0; i < N_ADDRSETS; i++)
    {
      const int j = (i + t + 1) % N_ADDRSETS;
      copy_addrset_into_addrset_uc (g_gv, g_as[i], g_as[j]);
      copy_addrset_into_addrset_mc (g_gv, g_as[i], g_as[j]);
    }
    note_progress ();
  }
}

/* Copying an addrset into ITSELF is the degenerate same-stripe case, and it is
   reachable in cyclone (copy_addrset_into_addrset with as == asadd). */
static void body_self_copy (int t)
{
  (void) t;
  for (int round = 0; round < N_ROUNDS; round++)
  {
    for (int i = 0; i < N_ADDRSETS; i++)
      copy_addrset_into_addrset (g_gv, g_as[i], g_as[i]);
    note_progress ();
  }
}

/* ---- hazard 2: callback re-enters the addrset layer ----------------------- */

/* Shaped like purge_helper: a callback that operates on a DIFFERENT addrset,
   which takes that addrset's stripe. Deadlocks if forall still holds a lock. */
static void reentrant_count_cb (const ddsi_xlocator_t *loc, void *varg)
{
  (void) loc;
  (void) addrset_count ((struct addrset *) varg);
  note_callback_hit ();
}

/* One level deeper: the callback runs a whole nested forall over the partner. */
static void nested_forall_cb (const ddsi_xlocator_t *loc, void *varg)
{
  (void) loc;
  struct addrset *partner = varg;
  addrset_forall (partner, reentrant_count_cb, partner);
  note_callback_hit ();
}

static void body_reentrant_callback (int t)
{
  for (int round = 0; round < N_ROUNDS; round++)
  {
    for (int i = 0; i < N_ADDRSETS; i++)
    {
      struct addrset *partner = g_as[(i + t + 1) % N_ADDRSETS];
      addrset_forall (g_as[i], reentrant_count_cb, partner);
    }
    note_progress ();
  }
}

static void body_nested_forall (int t)
{
  for (int round = 0; round < N_ROUNDS / 4; round++)
  {
    for (int i = 0; i < N_ADDRSETS; i++)
    {
      struct addrset *partner = g_as[(i + t + 1) % N_ADDRSETS];
      addrset_forall (g_as[i], nested_forall_cb, partner);
    }
    note_progress ();
  }
}

/* ---- the restructuring must not have broken the semantics ----------------- */

static void counting_cb (const ddsi_xlocator_t *loc, void *varg)
{
  (void) loc;
  (*(size_t *) varg)++;
}

/* addrset_forall_count returns a count AND invokes the callback; after the
   snapshot rewrite those two must still agree, and both must match
   addrset_count. A mismatch would mean the snapshot dropped or duplicated
   locators. 17 locators also exceeds the 16-entry stack buffer, so the heap
   fallback is covered. */
static void phase_snapshot_is_faithful (void)
{
  struct addrset *as = new_addrset ();
  if (as == NULL)
    fail ("new_addrset returned NULL");
  for (int i = 0; i < 17; i++)
  {
    ddsi_xlocator_t x = make_locator (9000 + i);
    add_xlocator_to_addrset (g_gv, as, &x);
  }

  const size_t expected = addrset_count (as);
  if (expected != 17)
  {
    fprintf (stderr, "FAIL: addrset_count = %zu, expected 17\n", expected);
    exit (1);
  }

  size_t invoked = 0;
  const size_t returned = addrset_forall_count (as, counting_cb, &invoked);
  if (returned != expected || invoked != expected)
  {
    fprintf (stderr,
             "FAIL: forall_count returned %zu and invoked the callback %zu times, "
             "expected %zu for both\n",
             returned, invoked, expected);
    exit (1);
  }
  unref_addrset (as);
  printf ("  snapshot count/callback agreement (incl. heap fallback): ok\n");
}

/* Copy must still be a union, and idempotent. */
static void phase_copy_still_unions (void)
{
  struct addrset *a = new_addrset ();
  struct addrset *b = new_addrset ();
  for (int i = 0; i < 5; i++)
  {
    ddsi_xlocator_t x = make_locator (200 + i);
    add_xlocator_to_addrset (g_gv, a, &x);
  }
  for (int i = 0; i < 7; i++)
  {
    /* seeds 203..209: two overlap with a's 200..204, five are new */
    ddsi_xlocator_t x = make_locator (203 + i);
    add_xlocator_to_addrset (g_gv, b, &x);
  }
  const size_t before = addrset_count (a);
  copy_addrset_into_addrset (g_gv, a, b);
  const size_t after = addrset_count (a);
  if (before != 5 || after != 10)
  {
    fprintf (stderr, "FAIL: copy union wrong — before %zu (want 5), after %zu (want 10)\n",
             before, after);
    exit (1);
  }
  copy_addrset_into_addrset (g_gv, a, b);
  if (addrset_count (a) != after)
  {
    fprintf (stderr, "FAIL: second copy was not idempotent — %zu, want %zu\n",
             addrset_count (a), after);
    exit (1);
  }
  unref_addrset (a);
  unref_addrset (b);
  printf ("  copy still unions and is idempotent: ok\n");
}

/* ---- pool lifecycle ------------------------------------------------------- */

static void build_pool (void)
{
  for (int i = 0; i < N_ADDRSETS; i++)
  {
    g_as[i] = new_addrset ();
    if (g_as[i] == NULL)
      fail ("new_addrset returned NULL");
    for (int k = 0; k < LOCATORS_PER_ADDRSET; k++)
    {
      ddsi_xlocator_t x = make_locator (i * LOCATORS_PER_ADDRSET + k);
      add_xlocator_to_addrset (g_gv, g_as[i], &x);
    }
    if (addrset_count (g_as[i]) != (size_t) LOCATORS_PER_ADDRSET)
      fail ("addrset did not accept its seed locators — the gv/locator setup is wrong "
            "and every phase would be walking empty addrsets, proving nothing");
  }
}

static void free_pool (void)
{
  for (int i = 0; i < N_ADDRSETS; i++)
  {
    unref_addrset (g_as[i]);
    g_as[i] = NULL;
  }
}

int main (void)
{
  /* Zeroed: ddsi_is_mcaddr finds no transport factory and reports "unicast". */
  g_gv = calloc (1, sizeof (*g_gv));
  if (g_gv == NULL)
    fail ("could not allocate ddsi_domaingv");

  printf ("addrset striped-lock concurrency (%d addrsets, %d threads, %d rounds)\n",
          N_ADDRSETS, N_THREADS, N_ROUNDS);

  phase_snapshot_is_faithful ();
  phase_copy_still_unions ();

  /* Each phase gets a freshly built pool. The copy phases deliberately union
     every addrset into every other, so by the end each holds all
     N_ADDRSETS * LOCATORS_PER_ADDRSET locators — leaving that in place made the
     nested-forall phase quadratic in the union size (~10^8 inner callbacks) and
     it blew the deadline while making perfectly good progress. Phases that must
     be independent should say so by construction. */
  build_pool ();
  run_phase ("self-copy (same addrset, therefore same stripe)", body_self_copy);
  free_pool ();

  build_pool ();
  run_phase ("bidirectional copy under stripe collisions", body_bidirectional_copy);
  free_pool ();

  build_pool ();
  run_phase ("forall with a callback that locks another addrset", body_reentrant_callback);
  if (callback_hits () == 0)
    fail ("the re-entrant callback never fired — the addrsets were empty, so this "
          "phase proved nothing");
  free_pool ();

  build_pool ();
  run_phase ("forall with a callback that runs a nested forall", body_nested_forall);
  if (callback_hits () == 0)
    fail ("the nested-forall callback never fired");
  free_pool ();

  free (g_gv);

  printf ("PASS\n");
  return 0;
}
