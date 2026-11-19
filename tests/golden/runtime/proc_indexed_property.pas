program ProcIndexedProperty;

// A procedural value read through an indexed or a default property whose reader is a function
// is called when written without parentheses outside a value position, as a property without
// an index is (objects.md 9.4): the reader runs, then the value it returns.

type
  TFn = reference to function: Integer;

  THolder = class
    function GetFn(I: Integer): TFn;
    property Fns[I: Integer]: TFn read GetFn; default;
  end;

function Seven: Integer;
begin
  Result := 7;
end;

function THolder.GetFn(I: Integer): TFn;
begin
  WriteLn('read ', I);
  Result := Seven;
end;

var
  H: THolder;
  N: Integer;
  F: TFn;
begin
  H := THolder.Create;
  N := H.Fns[1];
  WriteLn(N);
  N := H[2];
  WriteLn(N);
  WriteLn(H.Fns[3]);
  F := H.Fns[4];
  WriteLn(F());
  WriteLn(H.Fns[5]());
  H.Free;
end.
