program AttrNames;

// How the name of an attribute resolves (attributes.md 2, 5): `Attribute` is added first, so
// `[Json]` uses `JsonAttribute`; E3402 for a name that is not an attribute class (a variable, a
// class that does not inherit from `TCustomAttribute`, and the name with `Attribute` added is the
// one reported when both are found); W0025 for a name found nowhere, whose arguments are not
// checked (no E0700); an attribute class declared in a later `type` section is not seen (W0025).
// The attributes of the testing framework are E3200 until M5, even when a routine has the name,
// unless an attribute class of that name is declared. A helper has no fields, so no `[weak]`
// (E0753).

type
  JsonAttribute = class(TCustomAttribute)
  end;
  Json = class
  end;
  PlainAttribute = class
  end;
  Plain = class
  end;
  TNode = class(TRefObject)
  end;
  TNodeHelper = class helper for TNode
    class var [weak] Last: TNode;
  end;
  SetupAttribute = class(TCustomAttribute)
  end;

var
  Flag: Integer;

type
  [Json] TA = class
  end;
  [Flag] TB = class
  end;
  [Plain] TC = class
  end;
  [Missing(Undeclared + 1)] TD = class
  end;
  [Later] TE = class
  end;

type
  LaterAttribute = class(TCustomAttribute)
  end;

[Test]
procedure Test;
begin
end;

[Setup]
procedure Prepare;
begin
end;

[TestCase(1, 2)]
procedure Pair;
begin
end;

begin
  Flag := 0;
  Test;
  Prepare;
  Pair;
  WriteLn(Flag);
end.
