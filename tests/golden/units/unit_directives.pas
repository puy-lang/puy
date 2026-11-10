// Conditional directives around the sections of a unit and between its declarations are at a
// boundary (no W0005; docs/spec/directives.md 3.1).
unit UnitDirectives;

{$IFDEF NEVER}
{$ENDIF}
interface
{$IFDEF NEVER}
uses Nothing;
{$ENDIF}

const
  A = 1;
{$IFDEF NEVER}
procedure Hidden;
{$ENDIF}
procedure Shown; {$IFDEF NEVER} inline; {$ENDIF}

{$IFDEF NEVER}
{$ENDIF}
implementation
{$IFDEF NEVER}
{$ENDIF}

procedure Shown;
begin
end;

{$IFDEF NEVER}
{$ENDIF}
end.
