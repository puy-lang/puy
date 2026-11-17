program DistinctManaged;

// A distinct type of a managed type (types.md 1.3) holds its value as the root does: strings of
// `THtml` in variables, fields, elements, parameters and an instantiation are counted and
// released (no leak).

type
  THtml = type string;
  TBytes8 = type UTF8String;

  TPage = record
    Title: THtml;
    Body: TBytes8;
  end;

  TBox<T> = class
    Value: T;
  end;

function Wrap(const H: THtml; Tag: THtml): THtml;
begin
  Result := '<' + Tag + '>' + H + '</' + Tag + '>';
end;

var
  P: TPage;
  A: array of THtml;
  Box: TBox<THtml>;
  H: THtml;
begin
  H := 'x';
  H := H + H;
  P.Title := Wrap(H, 'b');
  P.Body := 'body';
  SetLength(A, 2);
  A[0] := P.Title;
  A[1] := A[0] + '!';
  Box := TBox<THtml>.Create;
  Box.Value := A[1];
  WriteLn(P.Title, ' ', P.Body, ' ', A[1], ' ', Box.Value, ' ', Length(A));
  Box.Free;
end.
