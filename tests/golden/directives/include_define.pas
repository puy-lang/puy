program IncludeDefine;

{$I include_define.inc}

begin
  {$IFDEF FEATURE}
  WriteLn('feature on');
  {$ELSE}
  WriteLn('feature off');
  {$ENDIF}
  {$I include_define.off.inc}
  {$IFDEF FEATURE}
  WriteLn('still on');
  {$ELSE}
  WriteLn('now off');
  {$ENDIF}
end.
