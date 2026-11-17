program HintDirectives;

// Hint directives do not change what a program does (attributes.md 6): a `deprecated` routine,
// an `experimental` method, and a type, a constant and a variable marked `deprecated` run as
// without the marks. The warnings are turned off with the names Delphi and FPC also know
// (diagnostics.md 5), since a case of diff/ compiles without diagnostics.

{$WARN SYMBOL_DEPRECATED OFF}
{$WARN SYMBOL_EXPERIMENTAL OFF}

type
  TOld = Integer deprecated;
  TCounter = class
    Count: Integer;
    procedure Bump; experimental;
  end;

const
  Step = 2 deprecated;

var
  Total: Integer = 0 deprecated;

procedure TCounter.Bump;
begin
  Count := Count + Step;
end;

procedure OldSum(A, B: Integer); deprecated 'use Sum';
begin
  WriteLn(A + B);
end;

var
  C: TCounter;
  X: TOld;
begin
  OldSum(1, 2);
  C := TCounter.Create;
  C.Bump;
  C.Bump;
  X := C.Count;
  Total := Total + X;
  WriteLn(Total);
  C.Free;
end.
