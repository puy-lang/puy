program DivInt64Overflow;

uses System.SysUtils;

// Low(Int64) div -1 does not fit in Int64: under {$Q+} it raises EIntOverflow
// (docs/spec/bytecode.md 2.2, `DivIntChk`), caught here and then left uncaught.

function Divide(A, B: Int64): Int64;
begin
  Result := A div B;
end;

var
  M: Int64;
begin
  M := Low(Int64);
  try
    WriteLn(Divide(M, -1));
  except
    on E: EIntOverflow do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  WriteLn(Divide(M, 2));
  WriteLn(Divide(M, -1));
  WriteLn('not reached');
end.
