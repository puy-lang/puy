program AnonymousClassType;

// A `class` / `interface` type written where a type is expected (grammar.md 1.3): E0404
// in each position, skipping the type up to its `end` or to the end of the declaration.

type
  TFoo = class
  end;
  TRec = record
    F: class end;                       // E0404
    G: Integer;
  end;

var
  X: class end;                         // E0404
  Y: interface end;                     // E0404
  C: class of TFoo;                     // E0404
  Z: class(TFoo) end;                   // E0404
  W: array[0..1] of interface end;      // E0404

procedure P(A: class; B: Integer);      // E0404: parameter
begin
end;

procedure Q(A: class of TFoo);          // E0404: parameter
begin
end;

function F: interface;                  // E0404: return type
begin
end;

function G: class(TFoo) end;            // E0404: return type
begin
end;

begin
  var L: TArray<class of TFoo>;         // E0404
  var M: class end;                     // E0404
  for var K: class of TFoo in [] do;    // E0404 (no E0729 for `[]` on top)
  for var U: TMissing in [1, 2] do;     // E0700 (no E0729 for `[1, 2]` on top)
  for var J: class end := 0 to 1 do;    // E0404
  P(nil, 1);
end.
