# Structured model transport stabilization

OpenAI-compatible and Anthropic HTTP/configuration infrastructure is shared by
the citation assessor and manuscript claim extractor through
`nineprofs-structured-model`.

The semantic adapters remain independent: each owns its environment prefix,
prompt, structured-output schema, response parsing, validation, and public
error contract.
