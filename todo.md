Todo
====

Multiple Layouts
----------------

Add support for multiple layouts. Each is defined independantly in their own
file. Layouts are named in config file, including the starting layout, which
must be named "main". Support switching between them, e.g. user presses a button
to show symbols that are not in the main layout. `RawKey` gets new meta key to
switch to named layout.

Menu
----

Finish menu: nicer interface, config/options setting in menu, controller
navigation in move window

User Defined Input Mapping
--------------------------

Custom controller input mapping instead of hard coded (see code marked
allow(unused) in controller/mod.rs as very early start to this)
