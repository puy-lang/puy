// A bodyless declaration without forward (E0411) is treated as a forward declaration and bound to the later body
// (no chained E0701; grammar.md 1.3, semantics.md 4.1). A header directly followed by
// begin / const / var / procedure / function is read as having a body (including nested routines),
// so the bodyless declarations here are separated by script top-level statements

procedure Later;
WriteLn('a');
procedure Never;
WriteLn('b');
procedure Mismatch(X: Integer);
WriteLn('c');
procedure Twice;
WriteLn('d');
procedure Twice;
WriteLn('e');
procedure Fwd; forward;
procedure FwdThenBodyless; forward;
procedure FwdThenBodyless;
WriteLn('f');
procedure Later; begin WriteLn('later'); end;
procedure Mismatch; begin end;
procedure Twice; begin end;
Later;
