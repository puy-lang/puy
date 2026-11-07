program NestedRoutine;

// A nested routine without a var section is also E3200 (grammar.md 1.3). A procedure /
// function right after the header starts the outer body; it is not a bodyless declaration (E0411)

procedure P;
  procedure Inner;
  begin
  end;
begin
  Inner;
end;

// No additional E3200 for a routine nested inside a nested routine
procedure A;
  procedure B;
    function C: Integer;
    begin
      Result := 1;
    end;
  begin
  end;
begin
end;

begin
  P;
  A;
end.
