# Adoption probe brief: field solutions

**Run by a person, not a model.** This brief is for a technician or a student
who has written small Arduino or Python programs and has used a terminal, but
has not built firmware with cmake, cargo or west.

## What you get

- A laptop running Ubuntu 22.04 or 24.04, with internet.
- One supported board in its unopened box, and a USB cable. Before the run,
  the organiser picks a board whose support level in the book is "Verified" or
  "Boots", and writes its name here: ____________.
- The nano-ros book, open at the start: <https://newslabntu.github.io/nano-ros-book/>.
- A stopwatch.

Nothing else: no help from someone who knows nano-ros, and no other website
unless the book links to it.

## The task

Start the stopwatch when you open the box. Following only the book:

1. Install what the book says to install.
2. Build the first example for your board.
3. Flash it to the board.
4. See its messages arrive on the laptop with `ros2 topic echo` (the book shows
   how).
5. Change the message text or the publish rate, rebuild, flash, and see the
   change.

Stop the stopwatch when step 5 shows the change.

## Budget

**30 minutes from the unopened box to step 4.** Write down the time at the end
of each step.

## While you work

Every time you get stuck, write it down before you try to get unstuck:

- which page and section you were on;
- what you typed;
- what the screen said (copy the last few lines, or take a photo);
- how long you were stuck, and what got you going again.

Also note any word you did not understand, even if you guessed right.

## After

Hand the notes and the step times to the organiser, who files each stuck point
as an issue with `purpose: field` (see [README.md](README.md#filing)).
