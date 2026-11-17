program AttrTrailingComma;

// A `,` right before the `]` of an attribute list is E0405, expecting another attribute name
// (attributes.md 1.1).

type
  MarkAttribute = class(TCustomAttribute)
  end;

  [Mark, ] TX = class
  end;

begin
end.
