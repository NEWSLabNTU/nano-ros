# Adoption probe brief: industrial

You are a software engineer at a company that builds safety-relevant embedded
products. Your team is evaluating nano-ros for a controller that must talk to a
ROS 2 system. You have not used it before. You will be asked to defend the
choice to a safety assessor and a security reviewer.

**Your only source is the nano-ros book.** Do not read the source repository,
its design documents or its issue tracker. If the book sends you to one of
them, note that as a stop.

## Host

A pristine `ubuntu:24.04` container with no network after the install step
(see task 3). Work as a non-root user. You may use `apt` only when the book
tells you to.

## Tasks, in order

1. **Install and build the first example the book recommends**, on QEMU, from
   the installation chapter onward. Record the exact versions of every tool the
   install brought in, and where each came from.
2. **Reproduce the image.** Build the same example a second time from a clean
   directory and compare the two images. Report whether they are byte-identical,
   and if not, whether the book says why.
3. **Install offline.** In a second container with the network disabled
   (`docker run --network none`), using only files you copied in, repeat task 1
   up to the first output line. Note every step that tried to reach the network.
4. **Produce the evidence a reviewer would ask for**, from the book alone:
   - the support level of the board you used, what it is measured on and when
     it was last run;
   - the static memory the image uses, and the heap and stack budgets;
   - the list of third-party code compiled into the image, and every patch
     nano-ros carries on top of an upstream project;
   - one document you could attach to the review that states the toolchain,
     the pins and the board's support level (the book may call it a report).
5. **Ask the long-term question.** Find what the book says about pins, release
   support and backports. Note what is missing for a product that ships for
   five years.

## Budget

Reproducible, auditable, offline-capable. There is no time limit, but any task
the book cannot answer is a stop, and any answer that needs the source tree is
a stop.

## Report

A run log (commands, decisive output lines, timestamps) and a numbered list of
stops in the form described in [README.md](README.md#what-counts-as-a-stop).
For each task, say done, done with stops, or not possible from the book.
