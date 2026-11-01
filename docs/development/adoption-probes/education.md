# Adoption probe brief: education

You are an instructor at an engineering school preparing a ROS 2 embedded lab
for next semester. Thirty students will repeat each lab on lab machines and
their own laptops. The lab is two hours and must not need hardware. You are
deciding whether nano-ros can carry the course. You have taught ROS 2 on Linux
before; you have not used nano-ros.

**Your only source is the nano-ros book.** Do not read the source repository,
its design documents or its issue tracker. If the book sends you to one of
them, note that as a stop.

## Host

A pristine `ubuntu:24.04` container, standing in for one student seat. Work as a
non-root user. For task 4, start a second container from the same image.

## Tasks, in order

1. **Install on a seat**, from the installation chapter, and time it from the
   first command to the first output of the first QEMU example. The seat's
   budget is 15 minutes, or a lab image the book tells you how to prepare.
2. **Plan a semester's labs from the book.** For each of: publish and
   subscribe, QoS, a service, parameters, lifecycle, real-time tasks and
   timing, talking to a ROS 2 graph on the host, find the chapter or example
   that teaches it on an emulator. Note which have no such chapter.
3. **Write the grading line for one lab.** For the first example, find the
   exact output the book promises, and a command an instructor could run per
   seat that passes when that output appears and fails otherwise.
4. **Pin the semester.** Find how the book fixes the toolchain version so a
   seat that installs in week 10 gets what week 1 got. Install a second seat
   with that pin and confirm both seats print the same output for the first
   example.
5. **Imagine the first question a student pastes into a search engine.** Take
   the first error you met in tasks 1 to 4 and say whether its message alone
   tells a student what to type next.

## Budget

15 minutes to install a seat, a lab that fits two hours, and the same result
on every seat. Measure the install time; a step that needs hardware or an
internal word the course would have to teach is a stop.

## Report

A run log (commands, decisive output lines, timestamps), the install time, the
lab coverage table from task 2, and a numbered list of stops in the form
described in [README.md](README.md#what-counts-as-a-stop). For each task, say
done, done with stops, or not possible from the book.
