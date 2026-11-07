program DirUnknown;
{$RANGECHEKS ON}
{$100 yen}
{$IFC X}
{$H-}
{$LONGSTRINGS OFF}
{$ZEROBASEDSTRINGS ON}
{$I-}
{$POINTERS ON}
{$POINTERMATH ON}
{$POINTERS OFF}
{$POINTERMATH OFF}
{$DEFINE DEBUG}
{$UNDEF RELEASE}
{$UNDEF PUY}
{$DEFINE 1x}
{$I}
{$I ''}
{$I %DATE%}
{$IFOPT B+}
{$ENDIF}
{$IFOPT R+}
{$DEFINE R_ON}
{$ENDIF}
{$IFOPT H+}
{$DEFINE H_ON}
{$ENDIF}
{$Q yes}
{$WARN unused_variable OFF}
{$WARN SYMBOL_DEPRECATED OFF}
{$WARN nosuch ON}
{$WARN Unused-Variable error}
{$MESSAGE ERROR 'boom'}
{$MESSAGE FATAL 'fatal boom'}
{$MESSAGE WARN 'careful'}
{$MESSAGE 'just a hint'}
{$MESSAGE HINT 'hint 2'}
{$ERROR fpc error}
{$WARNING fpc warning}
{$NOTE fpc note}
{$INFO 'fpc info'}
{$R+,Q+}
{$R-,Q-,C-}
{$MODE DELPHI}
{$MODE DELPHIUNICODE}
{$REGION 'r'}
{$ENDREGION}
{$H+}
{$LEGACYIFEND ON}
{$INLINE AUTO}
{$IFDEF R_ON}
{$ENDIF DEBUG}
{$IFNDEF H_ON}
{$MESSAGE ERROR 'H must be on'}
{$ENDIF}
procedure P;
var
  Unused: Integer;
begin
end;
begin
  P;
  WriteLn('x');
end.
