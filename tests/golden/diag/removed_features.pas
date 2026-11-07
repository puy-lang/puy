program RemovedFeatures;

procedure Ext; external 'user32.dll' name 'MessageBoxW';
procedure Conv; stdcall; safecall; varargs; platform; library; inline; assembler;
begin
end;
procedure Method.Click; begin end;

label L;
var
  P: Pointer;
  Q: AnsiString;
  R: Text;
  V: Variant;
  W: Real48;
  N: Integer;
  Z: Integer absolute N;
begin
  N := 1;
  with P do N := 2;
  goto L;
  Z := Integer(P^);
  Z := @N;
  asm nop end;
  New(P);
  Dispose(P);
  WriteLn(N, Z);
end.
