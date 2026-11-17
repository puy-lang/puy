program DumpAttrMembers;

// Attributes on the members of an interface (the first member too: `[` not followed by a
// string literal is not a GUID; objects.md 7.1), of a helper, and on a `class operator`
// (attributes.md 1.2) are kept like those of a class's methods and properties.

type
  MarkAttribute = class(TCustomAttribute)
  end;

  IThing = interface
    [Mark] procedure Run;
    [Mark] function GetN: Integer;
    [Mark] property N: Integer read GetN;
  end;

  IOther = interface(IThing)
    ['{01234567-89AB-CDEF-0123-456789ABCDEF}']
    [Mark] procedure Go;
  end;

  TV = record
    X: Integer;
    [Mark] class operator Add(const A, B: TV): TV;
  end;

  TIntHelper = record helper for Integer
    [Mark] function Twice: Integer;
  end;

class operator TV.Add(const A, B: TV): TV;
begin
  Result := Default(TV);
  Result.X := A.X + B.X;
end;

function TIntHelper.Twice: Integer;
begin
  Result := Self * 2;
end;

var
  A, B: TV;
begin
  A.X := 1;
  B.X := 2;
  WriteLn((A + B).X, ' ', (3).Twice);
end.
