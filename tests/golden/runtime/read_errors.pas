program ReadErrors;

// `EInOutError` of `Read` / `ReadLn` (semantics.md 6.2): a bad number (a failed `ReadLn` drops
// the rest of its line, so reading again goes on with the next line), something after the last
// number, a bad real, and the end of input for a number and for a `Char`. A `Char` last in
// `ReadLn` does not check the rest of the line.

uses System.SysUtils;

var
  A, B, N: Integer;
  C: Char;
  R: Double;
begin
  repeat
    try
      ReadLn(N);
      Break;
    except
      on E: EInOutError do WriteLn(E.Message, ' ', E.ErrorCode);
    end;
  until False;
  WriteLn('N=', N);
  try
    ReadLn(A, B);
  except
    on E: EInOutError do WriteLn(E.Message);
  end;
  ReadLn(A, C);
  WriteLn(A, '[', C, ']');
  try
    ReadLn(R);
  except
    on E: EInOutError do WriteLn(E.Message);
  end;
  try
    Read(N);
  except
    on E: EInOutError do WriteLn(E.Message);
  end;
  try
    Read(C);
  except
    on E: EInOutError do WriteLn(E.Message);
  end;
  ReadLn(Input);
  WriteLn('done');
end.
