program NestedRoutine;

// A procedure / function right after the header is a nested routine that starts the outer body
// (grammar.md 1.3, semantics.md 4.9); it is not a bodyless declaration (E0411)

procedure P;
  procedure Inner;
  begin
  end;
begin
  Inner;
end;

// A routine nested inside a nested routine
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
