program DirCond;
{$ENDIF}
{$ELSE}
{$ELSEIF True}
{$IFDEF PUY}
{$ELSE}
{$ELSE}
  this is skipped
{$ELSEIF True}
{$ENDIF}
{$IF 1 +}
{$ENDIF}
{$IF 99999999999999999999 > 1}
{$ENDIF}
{$IF #13 = 'a'}
{$ENDIF}
{$IF Foo}
{$ENDIF}
{$IF Level > 1}
{$ENDIF}
{$IF Foo and Defined(PUY)}
{$ENDIF}
{$IF Declared(Foo)}
{$ENDIF}
{$IF CompilerVersion >= 23}
{$ENDIF}
{$IF CompilerVersion < 23}
{$ELSEIF 23 <= RTLVersion}
{$ENDIF}
{$IF CompilerVersion = 23}
{$ENDIF}
{$IF Defined(X) and (CompilerVersion >= 23)}
{$ENDIF}
{$IF 1 + 'a'}
{$ENDIF}
{$IF 1 div 0 = 0}
{$ENDIF}
{$IF 1 mod (2 - 2) = 0}
{$ENDIF}
{$IF not 1}
{$ENDIF}
{$IFDEF Foo-bar}
{$ENDIF}
{$IF Defined(Foo-bar)}
{$ENDIF}
{$IFNDEF MSWINDOWS}
{$ENDIF}
{$IFDEF LINUX}
{$ENDIF}
{$IF Defined(CPUX64) or Defined(FPC)}
{$ENDIF}
{$IFDEF VER350}
{$ENDIF}
{$IFDEF DCC}
{$ENDIF}
{$IFDEF FPC}
{$ENDIF}
{$DEFINE MSWINDOWS}
{$IFDEF MSWINDOWS}
{$ENDIF}
{$IFDEF NOPE}
{$IF 1 +}
{$IF Declared(Foo)}
{$ENDIF}
{$ENDIF}
{$IF ('a' < 'b') and (PuyVersion >= 0) and (PuyVersionString <> '') and (not False) and (1 + 2 * 3 - -1 = 8) and (7 div 2 = 3) and (7 mod 2 = 1) and (1 xor 1 = 0) and (3 and 1 = 1) and (2 or 1 = 3) and ($10 = 16) and (%11 = 3) and (1_0 = 10) and ('A' <> 'a') and (2 >= 2) and (1 <= 2) and (1 < 2) and (2 > 1)}
{$ELSE}
{$MESSAGE ERROR 'expression evaluated wrongly'}
{$ENDIF}
begin
  WriteLn('x');
end.
