A C++ action client no longer guesses from the first bytes of a feedback or
result payload whether it already carries a CDR header: the executor always
supplies one, so the payload is forwarded as is, and a payload whose first word
happens to read as a header is no longer at risk of being misframed.
