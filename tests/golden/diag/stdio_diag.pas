program StdioDiag;

// Diagnostics of the standard I/O routines (semantics.md 6.1, 6.2): E0740 for `Input` /
// `Output` / `ErrOutput` anywhere but the first argument, E0702 for the wrong file or a value
// that is not a file, the E0718 variant for a type that cannot be read, E0705 for `Read` without
// a variable and for the argument counts of `Flush` and `Eof`.

type
  TColor = (Red, Green);

procedure P(X: Integer);
begin
end;

var
  B: Boolean;
  K: TColor;
  I: Integer = 0;
begin
  var F := Output;
  P(Input);
  Write(I, Output);
  Output;
  ErrOutput(1);
  I := System.Input;
  WriteLn(Input, 'x');
  ReadLn(Output, I);
  ReadLn(B);
  Read(K);
  Read;
  Read(Input);
  Flush;
  Flush(Input);
  Flush(I);
  Flush(Output, ErrOutput);
  B := Eof(Output);
  B := Eoln(I);
  B := Eof(Input, Input);
end.
